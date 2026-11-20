const raw = import.meta.env.VITE_API_BASE_URL;

if (!raw) {
  throw new Error(
    'VITE_API_BASE_URL is not set. Add it to web/.env.<mode> or pass it on the Vite command line.'
  );
}

try {
  new URL(raw);
} catch {
  throw new Error(`VITE_API_BASE_URL is not a valid URL: ${raw}`);
}

export const apiBaseUrl = raw.replace(/\/$/, '');

/**
 * Cloudflare Turnstile site key for the paid-API free-tier session (task 0277).
 * OPTIONAL: when unset the session layer is a no-op — no widget renders and a
 * 401 is not turned into a check, so the SPA runs against an un-armed backend. Set `VITE_TURNSTILE_SITE_KEY` (the
 * widget's PUBLIC site key) to arm, in lockstep with the backend `enableAuthLayer`.
 */
export const turnstileSiteKey: string | undefined =
  import.meta.env.VITE_TURNSTILE_SITE_KEY || undefined;
