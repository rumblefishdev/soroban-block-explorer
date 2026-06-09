// Free-tier session handling for the paid-API access layer (task 0277, see
// docs/paid-api/plan-platne-api.md). Flow: the SPA solves a Cloudflare Turnstile
// challenge → exchanges that token at `POST /auth/session` for a short-lived
// session JWT → attaches `Authorization: Bearer <jwt>` to every API call (wired
// in `./client.ts`). The JWT lives in memory only (never persisted) — a reload
// re-solves Turnstile, which is cheap in managed mode.
//
// GATED on `turnstileSiteKey`: when `VITE_TURNSTILE_SITE_KEY` is unset this
// whole module is inert (`ensureSessionToken` resolves to `null`, no widget
// loads), so the SPA runs unchanged against an un-armed backend. Arm the SPA in
// lockstep with the backend `enableAuthLayer`.

import { apiBaseUrl, turnstileSiteKey } from './config.js';

/** Mirrors the backend `SessionResponse` (crates/api/src/auth/mod.rs). */
interface SessionResponse {
  token: string;
  expires_in: number;
}

// In-memory session state. `expiresAtMs` is the absolute wall-clock expiry; we
// refresh a touch early so an in-flight request never rides an edge-expired JWT.
let sessionToken: string | null = null;
let expiresAtMs = 0;
const EXPIRY_SKEW_MS = 30_000;

// Single-flight: concurrent callers (e.g. a burst of queries on first paint)
// share one Turnstile solve + one `/auth/session` round-trip.
let inFlight: Promise<string | null> | null = null;

function isFresh(): boolean {
  return sessionToken !== null && Date.now() < expiresAtMs - EXPIRY_SKEW_MS;
}

/**
 * Resolve a usable session JWT, or `null` when the layer is disabled
 * (`turnstileSiteKey` unset). Reuses a fresh token; otherwise solves Turnstile
 * once and exchanges it. Never throws — on failure the token stays `null` and
 * the request proceeds without a Bearer (the backend decides the outcome).
 */
export async function ensureSessionToken(): Promise<string | null> {
  if (!turnstileSiteKey) return null;
  if (isFresh()) return sessionToken;
  if (inFlight) return inFlight;

  inFlight = (async () => {
    try {
      const turnstileToken = await solveTurnstile(turnstileSiteKey);
      const res = await fetch(`${apiBaseUrl}/auth/session`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token: turnstileToken }),
      });
      if (!res.ok) {
        invalidateSession();
        return null;
      }
      const body = (await res.json()) as SessionResponse;
      sessionToken = body.token;
      expiresAtMs = Date.now() + body.expires_in * 1000;
      return sessionToken;
    } catch {
      invalidateSession();
      return null;
    } finally {
      inFlight = null;
    }
  })();

  return inFlight;
}

/** Drop the cached JWT so the next request re-solves Turnstile. Called on 401. */
export function invalidateSession(): void {
  sessionToken = null;
  expiresAtMs = 0;
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

let scriptPromise: Promise<TurnstileApi> | null = null;

function loadTurnstileScript(): Promise<TurnstileApi> {
  if (window.turnstile) return Promise.resolve(window.turnstile);
  if (scriptPromise) return scriptPromise;

  scriptPromise = new Promise<TurnstileApi>((resolve, reject) => {
    const script = document.createElement('script');
    script.src = TURNSTILE_SRC;
    script.async = true;
    script.defer = true;
    script.onload = () => {
      if (window.turnstile) resolve(window.turnstile);
      else reject(new Error('Turnstile script loaded but window.turnstile is absent'));
    };
    script.onerror = () => {
      scriptPromise = null; // allow a later retry
      reject(new Error('Failed to load the Turnstile script'));
    };
    document.head.appendChild(script);
  });

  return scriptPromise;
}

/**
 * Render a one-shot managed widget and resolve with its token. For legitimate
 * traffic this never paints — Cloudflare resolves it silently and fires
 * `callback`. The container is a centered, top-layer overlay so that, on the
 * rare occasion Cloudflare escalates to an interactive challenge, the checkbox
 * is visible and clickable rather than hidden off-screen.
 */
async function solveTurnstile(sitekey: string): Promise<string> {
  const turnstile = await loadTurnstileScript();
  const container = document.createElement('div');
  container.style.cssText =
    'position:fixed;top:50%;left:50%;transform:translate(-50%,-50%);z-index:2147483647;';
  document.body.appendChild(container);

  return new Promise<string>((resolve, reject) => {
    let widgetId: string | undefined;
    const cleanup = () => {
      if (widgetId !== undefined) {
        try {
          turnstile.remove(widgetId);
        } catch {
          // widget already gone — ignore
        }
      }
      container.remove();
    };
    widgetId = turnstile.render(container, {
      sitekey,
      appearance: 'interaction-only',
      callback: (token) => {
        cleanup();
        resolve(token);
      },
      'error-callback': () => {
        cleanup();
        reject(new Error('Turnstile challenge failed'));
      },
      'expired-callback': () => {
        cleanup();
        reject(new Error('Turnstile token expired before use'));
      },
    });
  });
}
