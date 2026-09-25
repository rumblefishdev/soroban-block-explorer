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
   * Resolve a usable session JWT, or `null` when the layer is dark. Reuses a
   * fresh token; otherwise solves Turnstile once and exchanges it. Never
   * throws — on failure the token stays `null` and the request proceeds
   * without a Bearer (the backend decides the outcome).
   */
  ensureToken(): Promise<string | null>;
  /** Drop the cached JWT so the next request re-solves Turnstile. Called on 401. */
  invalidate(): void;
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
        const turnstileToken = await solve(siteKey);
        // Plain `fetch`, not the generated SDK: the SDK client's request
        // interceptor awaits `ensureToken`, so calling it here would wait on
        // its own in-flight promise forever.
        const res = await fetch(`${apiBaseUrl}/auth/session`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ token: turnstileToken }),
        });
        if (!res.ok) {
          invalidate();
          return null;
        }
        const body = (await res.json()) as SessionResponse;
        token = body.token;
        expiresAtMs = Date.now() + body.expires_in * 1000;
        return token;
      } catch {
        invalidate();
        return null;
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
// `ensureToken`. Reject + clean up after this long.
const TURNSTILE_TIMEOUT_MS = 20_000;

interface TurnstileApi {
  render(
    container: HTMLElement,
    opts: {
      sitekey: string;
      callback: (token: string) => void;
      'error-callback'?: () => void;
      'expired-callback'?: () => void;
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
        if (window.turnstile) resolve(window.turnstile);
        else
          reject(
            new Error('Turnstile script loaded but window.turnstile is absent')
          );
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
 * is visible and clickable rather than hidden off-screen.
 */
async function solveTurnstile(
  sitekey: string,
  loadScript: () => Promise<TurnstileApi>
): Promise<string> {
  const turnstile = await loadScript();
  const container = document.createElement('div');
  container.style.cssText =
    'position:fixed;top:50%;left:50%;transform:translate(-50%,-50%);z-index:2147483647;';
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

    timer = setTimeout(() => fail('Turnstile timed out'), TURNSTILE_TIMEOUT_MS);

    try {
      widgetId = turnstile.render(container, {
        sitekey,
        appearance: 'interaction-only',
        callback: succeed,
        'error-callback': () => fail('Turnstile challenge failed'),
        'expired-callback': () => fail('Turnstile token expired before use'),
      });
    } catch (e) {
      fail(e instanceof Error ? e.message : 'Turnstile render failed');
    }
  });
}
