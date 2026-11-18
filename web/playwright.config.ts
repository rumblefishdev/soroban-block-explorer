import { defineConfig, devices } from '@playwright/test';

/**
 * Browser regression for the account page (issue #377).
 *
 * The API is intercepted per test rather than reached: CI has no mTLS
 * certificate for production ClickHouse, and a test whose result depends on
 * live chain data reports the network's health, not the code's. What is left
 * is exactly what jsdom cannot check — real history, real layout, real
 * scrolling.
 *
 * `webServer` boots the app's own dev server. `reuseExistingServer` keeps a
 * local run instant when one is already up; CI always starts its own.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: 'http://localhost:4280',
    trace: 'on-first-retry',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command: 'pnpm exec vite --port 4280 --strictPort',
      url: 'http://localhost:4280',
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
      // `src/api/config.ts` THROWS at module load when this is unset, so without
      // it the app never boots and every assertion fails as "element not found"
      // — which reads like a broken page rather than a missing variable. The
      // suite passed locally only because `.env.development.local` supplies one,
      // and that file is untracked, so CI had none.
      //
      // The value is never fetched: every request is intercepted by `page.route`.
      // It only has to parse as a URL, and pointing it at the test server keeps a
      // stray un-mocked call local instead of aimed at a real host.
      env: { VITE_API_BASE_URL: 'http://localhost:4280/api' },
    },

    // Four more servers, one per Cloudflare dummy site key, for
    // `e2e/turnstile.spec.ts`. The key is read at build time, so each key
    // needs its own server; the tests then meet the real Turnstile widget,
    // loaded from challenges.cloudflare.com, exactly as production does.
    // Keys: https://developers.cloudflare.com/turnstile/troubleshooting/testing/
    // Never reused: a server already on the port would be some other code.
    ...[
      { port: 4281, siteKey: '1x00000000000000000000AA' }, // always passes
      { port: 4282, siteKey: '2x00000000000000000000AB' }, // always fails
      { port: 4283, siteKey: '3x00000000000000000000FF' }, // forces a challenge
      { port: 4284, siteKey: '2x00000000000000000000BB' }, // fails, invisible
    ].map(({ port, siteKey }) => ({
      command: `pnpm exec vite --port ${port} --strictPort`,
      url: `http://localhost:${port}`,
      reuseExistingServer: false,
      timeout: 120_000,
      env: {
        VITE_API_BASE_URL: `http://localhost:${port}/api`,
        VITE_TURNSTILE_SITE_KEY: siteKey,
      },
    })),
  ],
});
