// Free-tier session for the paid-API access layer (task 0277). Flow: the SPA
// solves a Cloudflare Turnstile challenge → posts that token to
// `POST /auth/session` → the API sets the session in the HttpOnly cookie
// `__Host-session` (task 0677). The browser sends it with every read, so one
// check serves every tab and reload until it expires; the SPA never sees it.
//
// A read goes out at once; `./client.ts` turns its 401 into `renew()` and one
// repeat. Wired only when `VITE_TURNSTILE_SITE_KEY` is set, in lockstep with
// the backend `enableAuthLayer`.
//
// All state lives in the object `createSession` returns — nothing mutable at
// module scope — so a test builds a fresh one (task 0510). The app holds one,
// in `./client.ts`.

import { colorsDark } from '@rumblefish/soroban-block-explorer-ui';

export interface SessionOptions {
  siteKey: string;
  apiBaseUrl: string;
  /** Solves a Turnstile challenge. Injectable so a test needs no widget. */
  solve?: (siteKey: string) => Promise<string>;
}

export interface Session {
  /** How many mints have finished in this tab. A read notes it when sent. */
  mints(): number;
  /**
   * Make the API set a fresh session cookie, after a read got a 401.
   * `seenMints` is `mints()` when that read was sent: if a mint finished
   * since, the read went out without the new cookie and only needs repeating.
   * Otherwise solves Turnstile once and mints; concurrent callers share it.
   * A failed solve or a refused mint shows the retry panel and waits for the
   * visitor. Resolves `false` once the browser is known to drop the cookie.
   */
  renew(seenMints: number): Promise<boolean>;
  /**
   * A read repeated right after a fresh mint still got a 401: the browser
   * does not keep the cookie. Shows one panel; `renew` stops checking.
   */
  cookiesBlocked(): void;
}

export function createSession({
  siteKey,
  apiBaseUrl,
  solve = createTurnstileSolver(),
}: SessionOptions): Session {
  let mints = 0;
  let blocked = false;
  // Single-flight: the burst of 401s on first paint shares one check.
  let inFlight: Promise<boolean> | null = null;

  const renew = (seenMints: number): Promise<boolean> => {
    if (blocked) return Promise.resolve(false);
    if (mints > seenMints) return Promise.resolve(true);
    if (inFlight) return inFlight;

    inFlight = (async () => {
      try {
        // Until a session is minted: solve, mint, and on failure wait for
        // the visitor's "Try again" before solving again.
        for (;;) {
          let reason: string;
          try {
            const turnstileToken = await solve(siteKey);
            // Plain `fetch`, not the generated SDK: the SDK's 401 handling
            // would call `renew` again from inside it.
            const res = await fetch(`${apiBaseUrl}/auth/session`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ token: turnstileToken }),
              // Without it the browser drops the cross-origin `Set-Cookie`.
              credentials: 'include',
            });
            if (res.ok) {
              mints += 1;
              return true;
            }
            reason = `The server did not accept the check (HTTP ${res.status}).`;
          } catch (e) {
            reason = e instanceof Error ? e.message : String(e);
          }
          console.warn(`Turnstile session failed: ${reason}`);
          await showRetryPanel();
        }
      } finally {
        inFlight = null;
      }
    })();

    return inFlight;
  };

  const cookiesBlocked = () => {
    if (blocked) return;
    blocked = true;
    console.warn(
      'The session cookie was not kept: the browser blocks cookies.'
    );
    // No check can help, so one card that stays, with nothing to click.
    showCard('alert', 'This site needs cookies');
  };

  return { mints: () => mints, renew, cookiesBlocked };
}

// ── Turnstile widget ────────────────────────────────────────────────────────
// We drive Turnstile programmatically (no JSX): load the script once, render an
// invisible one-shot widget into a detached container, and resolve with the
// token from its success callback. Managed mode stays invisible unless
// Cloudflare decides an interactive challenge is warranted.

const TURNSTILE_SRC =
  'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';

interface TurnstileApi {
  render(
    container: HTMLElement,
    opts: {
      sitekey: string;
      callback: (token: string) => void;
      'error-callback'?: (errorCode: string) => void;
      'expired-callback'?: () => void;
      'unsupported-callback'?: () => void;
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
 * is visible and clickable rather than hidden off-screen. Cloudflare's own
 * "Verify you are human" says what it wants, so nothing is added around it.
 */
async function solveTurnstile(
  sitekey: string,
  loadScript: () => Promise<TurnstileApi>
): Promise<string> {
  const turnstile = await loadScript();
  const container = document.createElement('div');
  // Without a width, a box placed at `left:50%` shrinks to half a phone screen
  // and squeezes the 300 px widget.
  container.style.cssText =
    PANEL_POSITION_CSS + 'width:calc(100vw - 32px);max-width:300px;';
  document.body.appendChild(container);

  return new Promise<string>((resolve, reject) => {
    // `let`, not `const`: `cleanup` (below) reads widgetId before it is
    // assigned — a forward reference prefer-const can't see.
    // eslint-disable-next-line prefer-const
    let widgetId: string | undefined;
    let settled = false;
    // Idempotent teardown: remove the widget + container.
    const cleanup = () => {
      if (settled) return;
      settled = true;
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

    // Cloudflare ends every check with one of these callbacks; a click it
    // asks for waits for the visitor as long as it takes.
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
        'unsupported-callback': () =>
          fail('Cloudflare Turnstile does not support this browser.'),
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
// The dark palette of the design system: the page around the card may be
// either theme, and one fixed card keeps the panel readable on both.
// `background-color`, not the `background` shorthand: the test DOM (cssstyle
// 5.3.7) drops the whole style when `color` follows that shorthand.
// A phone's width less 16 px a side, up to 360 px: without a width, a card
// placed at `left:50%` shrinks to half the screen.
const PANEL_CARD_CSS = `box-sizing:border-box;width:calc(100vw - 32px);max-width:360px;padding:20px 24px;border-radius:12px;border:1px solid ${colorsDark.stroke.default};background-color:${colorsDark.surface.grayMain};color:${colorsDark.text.primary};text-align:center;box-shadow:0 8px 32px rgba(0,0,0,0.4);font-family:inherit;`;
const PANEL_TEXT_CSS = 'margin:0 0 12px;font-size:14px;line-height:1.5;';

/** One card with a title, appended to the page. */
function showCard(role: string, titleText: string): HTMLElement {
  const panel = document.createElement('div');
  panel.setAttribute('role', role);
  panel.setAttribute('aria-labelledby', 'turnstile-panel-title');
  panel.style.cssText = PANEL_POSITION_CSS + PANEL_CARD_CSS;

  const title = document.createElement('p');
  title.id = 'turnstile-panel-title';
  title.textContent = titleText;
  title.style.cssText = PANEL_TEXT_CSS + 'font-weight:600;font-size:16px;';
  panel.append(title);
  document.body.appendChild(panel);
  return panel;
}

/**
 * A card that says the check failed, with a "Try again" button; the technical
 * reason goes only to the console. Resolves (and removes the card) on the
 * click; nothing is solved again before it, so an unattended tab starts no
 * new challenges.
 */
function showRetryPanel(): Promise<void> {
  // Named for screen readers, with focus on the button, so a
  // keyboard user is not left on a page whose sections only say "loading".
  const panel = showCard('alertdialog', "We couldn't verify your browser");
  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = 'Try again';
  button.style.cssText = `padding:8px 20px;border:0;border-radius:8px;background:${colorsDark.surface.primaryMain};color:${colorsDark.text.inverted};font:inherit;font-weight:600;cursor:pointer;`;

  panel.append(button);
  button.focus();

  return new Promise<void>((resolve) => {
    button.addEventListener('click', () => {
      panel.remove();
      resolve();
    });
  });
}
