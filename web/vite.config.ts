import react from '@vitejs/plugin-react';
import { dirname } from 'path';
import { fileURLToPath } from 'url';
import { defineConfig, loadEnv } from 'vite';

const root = dirname(fileURLToPath(import.meta.url));

export default defineConfig(({ mode }) => {
  // Load ALL env (the `''` prefix includes non-VITE_ vars) so the dev-proxy
  // settings below are readable in this Node config WITHOUT exposing them to the
  // client bundle (only `VITE_`-prefixed vars reach `import.meta.env`).
  const env = loadEnv(mode, root, '');

  // Local dev against the REAL (Cloudflare-fronted) API — Option B.
  // The browser only ever talks to localhost (same-origin → no CORS); Vite
  // forwards API calls to the prod host and injects a DEDICATED dev `x-api-key`
  // SERVER-SIDE here (never in the client bundle, never committed). Enabled only
  // when DEV_API_PROXY_TARGET is set in web/.env.development (gitignored);
  // otherwise no proxy and the app uses VITE_API_BASE_URL directly.
  // Pair it with VITE_API_BASE_URL=http://localhost:4200 so calls hit Vite.
  const proxyTarget = env.DEV_API_PROXY_TARGET;
  const devApiKey = env.DEV_API_KEY;
  const proxy = proxyTarget
    ? Object.fromEntries(
        ['/v1', '/health', '/api-docs'].map((path) => [
          path,
          {
            target: proxyTarget,
            changeOrigin: true, // rewrite Host so Cloudflare + API GW custom domain route correctly
            secure: true,
            ...(devApiKey ? { headers: { 'x-api-key': devApiKey } } : {}),
          },
        ])
      )
    : undefined;

  return {
    root,
    plugins: [react()],
    resolve: {
      conditions: ['soroban-block-explorer-source'],
    },
    server: {
      port: 4200,
      host: 'localhost',
      proxy,
    },
    build: {
      outDir: './dist',
      emptyOutDir: true,
      reportCompressedSize: true,
    },
  };
});
