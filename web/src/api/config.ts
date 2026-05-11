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
