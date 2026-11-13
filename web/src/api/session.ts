// Free-tier session handling for the paid-API access layer (task 0277, see
// docs/paid-api/plan-platne-api.md). Flow: the SPA solves a Cloudflare Turnstile
// challenge → exchanges that token at `POST /auth/session` for a short-lived
// session JWT → attaches `Authorization: Bearer <jwt>` to every API call (wired
// in `./client.ts`). The JWT lives in memory only (never persisted) — a reload
// re-solves Turnstile, which is cheap in managed mode.
//
// GATED on `siteKey`: when `VITE_TURNSTILE_SITE_KEY` is unset the session is
// inert (`ensureToken` resolves to `null`, no widget loads), so the SPA runs
// unchanged against an un-armed backend. Arm the SPA in lockstep with the
// backend `enableAuthLayer`.
//
// All state lives in the object `createSession` returns — nothing mutable at
// module scope — so a test builds a fresh one (task 0510). The app holds one,
// in `./client.ts`.

import type { SessionResponse } from '@rumblefish/api-types';

// Refresh a touch early so an in-flight request never rides an edge-expired JWT.
const EXPIRY_SKEW_MS = 30_000;

export interface SessionOptions {
  /** Turnstile site key; unset = the layer is dark. */
  siteKey: string | undefined;
  apiBaseUrl: string;
  /** Solves a Turnstile challenge. Injectable so a test needs no widget. */
  solve?: (siteKey: string) => Promise<string>;
}

export interface Session {
  /**
   * Resolve a usable session JWT, or `null` when the layer is dark (no site
   * key, or the API answers the mint with 404). Reuses a
   * fresh token; otherwise solves Turnstile once and exchanges it. Never
   * throws, and never resolves `null` while armed: a failed solve or a refused
   * exchange shows the retry panel and waits for the visitor, so no request
   * goes out without a session (task 0289 — a bare request gets a 401 that
   * the page can only show as a broken section).
   */
  ensureToken(): Promise<string | null>;
  /** Drop the cached JWT so the next request re-solves Turnstile. Called on 401. */
  invalidate(): void;
}

/**
 * True for the session-mint request itself. The request interceptor must not
 * attach a session to it: minting would wait on its own in-flight promise.
 */
export function isSessionMint(url: string): boolean {
  return new URL(url).pathname.endsWith('/auth/session');
}

export function createSession({
  siteKey,
  apiBaseUrl,
  solve = createTurnstileSolver(),
}: SessionOptions): Session {
  // The JWT lives in memory only (never persisted). `expiresAtMs` is the
  // absolute wall-clock expiry.
  let token: string | null = null;
  let expiresAtMs = 0;
  // Single-flight: concurrent callers (e.g. a burst of queries on first paint)
  // share one Turnstile solve + one `/auth/session` round-trip.
  let inFlight: Promise<string | null> | null = null;

  const invalidate = () => {
    token = null;
    expiresAtMs = 0;
  };

  const ensureToken = async (): Promise<string | null> => {
    if (!siteKey) return null;
    if (token !== null && Date.now() < expiresAtMs - EXPIRY_SKEW_MS) {
      return token;
    }
    if (inFlight) return inFlight;

    inFlight = (async () => {
      try {
        // Until a session is minted: solve, exchange, and on failure wait for
        // the visitor's "Try again" before solving again.
        for (;;) {
          let reason: string;
          try {
            const turnstileToken = await solve(siteKey);
            // Plain `fetch`, not the generated SDK: the SDK client's request
            // interceptor awaits `ensureToken`, so calling it here would wait
            // on its own in-flight promise forever.
            const res = await fetch(`${apiBaseUrl}/auth/session`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ token: turnstileToken }),
            });
            if (res.ok) {
              const body = (await res.json()) as SessionResponse;
              token = body.token;
              expiresAtMs = Date.now() + body.expires_in * 1000;
              return token;
            }
            // The API mounts `/auth/session` only with its gate on
            // (`crates/api/src/main.rs`); with the gate off a request needs no
            // session, so it goes out without one, as before task 0289.
            if (res.status === 404) return null;
            reason = `The server did not accept the check (HTTP ${res.status}).`;
          } catch (e) {
            reason = e instanceof Error ? e.message : String(e);
          }
          console.warn(`Turnstile session failed: ${reason}`);
          await showRetryPanel(reason);
        }
      } finally {
        inFlight = null;
      }
    })();

    return inFlight;
  };

  return { ensureToken, invalidate };
}

// ── Turnstile widget ────────────────────────────────────────────────────────
// We drive Turnstile programmatically (no JSX): load the script once, render an
// invisible one-shot widget into a detached container, and resolve with the
// token from its success callback. Managed mode stays invisible unless
// Cloudflare decides an interactive challenge is warranted.

const TURNSTILE_SRC =
  'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';

// Watchdog: if Turnstile never fires a callback (script loads but the widget
// stalls), solveTurnstile would hang forever and block every API call awaiting
// `ensureToken`. Reject + clean up after this long — but only while the check
// runs on its own: once Cloudflare asks the visitor to click, the widget waits
// for them as long as it takes.
const TURNSTILE_TIMEOUT_MS = 20_000;

interface TurnstileApi {
  render(
    container: HTMLElement,
    opts: {
      sitekey: string;
      callback: (token: string) => void;
      'error-callback'?: (errorCode: string) => void;
      'expired-callback'?: () => void;
      'before-interactive-callback'?: () => void;
      // 'manual' / 'never': Cloudflare must not restart a challenge on its own
      // (by default it re-arms a timed-out interactive check and retries a
      // failed one every 8 s) — an unattended tab then piles up unsolved
      // challenges. The visitor restarts it from the panel instead.
      'refresh-timeout'?: 'auto' | 'manual' | 'never';
      retry?: 'auto' | 'never';
      // 'interaction-only' pairs with the widget's "managed" mode (turnstile.tf):
      // the widget stays hidden and auto-resolves for legitimate traffic, and
      // only paints an interactive challenge when Cloudflare demands one.
      appearance?: 'always' | 'execute' | 'interaction-only';
    }
  ): string;
  remove(widgetId: string): void;
}

declare global {
  interface Window {
    turnstile?: TurnstileApi;
  }
}

/**
 * A `solve` for `createSession`: loads the Turnstile script at most once per
 * solver, then renders a one-shot widget per call.
 */
function createTurnstileSolver(): (sitekey: string) => Promise<string> {
  let scriptPromise: Promise<TurnstileApi> | null = null;

  const loadScript = (): Promise<TurnstileApi> => {
    if (window.turnstile) return Promise.resolve(window.turnstile);
    if (scriptPromise) return scriptPromise;

    scriptPromise = new Promise<TurnstileApi>((resolve, reject) => {
      const script = document.createElement('script');
      script.src = TURNSTILE_SRC;
      script.async = true;
      script.defer = true;
      script.onload = () => {
        if (window.turnstile) {
          resolve(window.turnstile);
        } else {
          scriptPromise = null; // allow a later retry
          reject(
            new Error('Turnstile script loaded but window.turnstile is absent')
          );
        }
      };
      script.onerror = () => {
        scriptPromise = null; // allow a later retry
        reject(new Error('Failed to load the Turnstile script'));
      };
      document.head.appendChild(script);
    });

    return scriptPromise;
  };

  return (sitekey) => solveTurnstile(sitekey, loadScript);
}

/**
 * Render a one-shot managed widget and resolve with its token. For legitimate
 * traffic this never paints — Cloudflare resolves it silently and fires
 * `callback`. The container is a centered, top-layer overlay so that, on the
 * rare occasion Cloudflare escalates to an interactive challenge, the checkbox
 * is visible and clickable rather than hidden off-screen; it then becomes a
 * card that says why the page is waiting.
 */
async function solveTurnstile(
  sitekey: string,
  loadScript: () => Promise<TurnstileApi>
): Promise<string> {
  const turnstile = await loadScript();
  const container = document.createElement('div');
  container.style.cssText = PANEL_POSITION_CSS;
  document.body.appendChild(container);

  return new Promise<string>((resolve, reject) => {
    // `let`, not `const`: `cleanup` (below) reads widgetId/timer before they are
    // assigned — a forward reference prefer-const can't see.
    // eslint-disable-next-line prefer-const
    let widgetId: string | undefined;
    let settled = false;
    // eslint-disable-next-line prefer-const
    let timer: ReturnType<typeof setTimeout> | undefined;
    // Idempotent teardown: clear the watchdog, remove the widget + container.
    const cleanup = () => {
      if (settled) return;
      settled = true;
      if (timer !== undefined) clearTimeout(timer);
      if (widgetId !== undefined) {
        try {
          turnstile.remove(widgetId);
        } catch {
          // widget already gone — ignore
        }
      }
      container.remove();
    };
    const succeed = (token: string) => {
      cleanup();
      resolve(token);
    };
    const fail = (message: string) => {
      cleanup();
      reject(new Error(message));
    };

    timer = setTimeout(
      () => fail('The check did not finish within 20 seconds.'),
      TURNSTILE_TIMEOUT_MS
    );

    // Cloudflare wants a click: stop the watchdog, turn the overlay into a
    // card and say why the page waits.
    const askForInteraction = () => {
      clearTimeout(timer);
      container.style.cssText = PANEL_POSITION_CSS + PANEL_CARD_CSS;
      const heading = document.createElement('p');
      heading.textContent = "Confirm you're human to keep loading data";
      heading.style.cssText = PANEL_TEXT_CSS;
      container.prepend(heading);
    };

    try {
      widgetId = turnstile.render(container, {
        sitekey,
        appearance: 'interaction-only',
        'refresh-timeout': 'manual',
        retry: 'never',
        callback: succeed,
        'error-callback': (errorCode) =>
          fail(`The check failed (Cloudflare Turnstile error ${errorCode}).`),
        'expired-callback': () => fail('The check expired before it was used.'),
        'before-interactive-callback': askForInteraction,
      });
    } catch (e) {
      fail(e instanceof Error ? e.message : 'Turnstile render failed');
    }
  });
}

// ── Panel ───────────────────────────────────────────────────────────────────
// One card at the centre of the screen, built with plain DOM like the widget
// container above: the session lives outside React, and the panel must show
// on any page while every data request waits for it.

const PANEL_POSITION_CSS =
  'position:fixed;top:50%;left:50%;transform:translate(-50%,-50%);z-index:2147483647;';
const PANEL_CARD_CSS =
  'max-width:360px;padding:20px 24px;border-radius:12px;background:#1e1e1e;color:#f5f5f5;text-align:center;box-shadow:0 8px 32px rgba(0,0,0,0.4);font-family:inherit;';
const PANEL_TEXT_CSS = 'margin:0 0 12px;font-size:14px;line-height:1.5;';

/**
 * The default `waitForRetry`: a card that says the check failed and why, with
 * a "Try again" button. Resolves (and removes the card) on the click; nothing
 * is solved again before it, so an unattended tab starts no new challenges.
 */
function showRetryPanel(reason: string): Promise<void> {
  const panel = document.createElement('div');
  panel.setAttribute('role', 'alertdialog');
  panel.style.cssText = PANEL_POSITION_CSS + PANEL_CARD_CSS;

  const title = document.createElement('p');
  title.textContent = "We couldn't verify your browser";
  title.style.cssText = PANEL_TEXT_CSS + 'font-weight:600;font-size:16px;';
  const detail = document.createElement('p');
  detail.textContent = `This explorer checks that visitors are human before it loads data. ${reason}`;
  detail.style.cssText = PANEL_TEXT_CSS;
  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = 'Try again';
  button.style.cssText =
    'padding:8px 20px;border:0;border-radius:8px;background:#fcd535;color:#111;font:inherit;font-weight:600;cursor:pointer;';

  panel.append(title, detail, button);
  document.body.appendChild(panel);

  return new Promise<void>((resolve) => {
    button.addEventListener('click', () => {
      panel.remove();
      resolve();
    });
  });
}
