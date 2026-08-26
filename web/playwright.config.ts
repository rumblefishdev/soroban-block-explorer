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
  webServer: {
    command: 'npx vite --port 4280 --strictPort',
    url: 'http://localhost:4280',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
