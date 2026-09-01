import { expect, test } from '@playwright/test';

/**
 * Task 0374 full-stack e2e — REAL data end to end: raw mainnet ledgers →
 * backfill-runner → local ClickHouse → the real API → this browser. No
 * route interception on purpose; the suite's other specs keep the mocked
 * idiom for CI. Gated on POOLS_REAL so CI (which has no local stack) skips.
 */
test.skip(!process.env.POOLS_REAL, 'needs the local real-data stack');

const SOROBAN_POOL = 'CC642QYWXXR2HUZDNJ6KYN5LV5JFPFPT4Q6YNKLZLYEFWZZZ5SJYLA5G';

test('soroban pool renders on the union list with legs and protocol', async ({
  page,
}) => {
  await page.goto('/liquidity-pools?kind=soroban');
  const row = page.locator('tr', { hasText: 'XLM / SHX' });
  await expect(row).toBeVisible();
  await expect(row.getByText('aquarius')).toBeVisible();
  await expect(row.getByText(/^CC64/)).toBeVisible();
});

test('soroban detail shows legs, chart, and its own activity feed', async ({
  page,
}) => {
  await page.goto(`/liquidity-pools/${SOROBAN_POOL}`);
  await expect(page.getByRole('heading', { name: 'XLM / SHX' })).toBeVisible();
  // Reserves from pool_state_changes, scaled by leg decimals (7).
  await expect(page.getByText('XLM reserve').first()).toBeVisible();
  await expect(page.getByText('SHX reserve').first()).toBeVisible();
  await expect(page.getByText('concentrated')).toBeVisible();
  // Chart section, fed from pool_state_changes + trade events + prices.
  await expect(page.getByRole('tab', { name: 'TVL' })).toBeVisible();
  // Activity fed from the pool's own soroban events: rows carry an event
  // chip and per-leg amounts labelled through the unified leg views.
  await expect(page.getByText('Recent activity')).toBeVisible();
  const tradeChip = page.getByText('Trade', { exact: true }).first();
  await expect(tradeChip).toBeVisible();
  // The full swap phrase lives on the amount stack's aria-label (the visual
  // text interleaves linked asset codes).
  await expect(
    page.getByLabel(/XLM → [\d,.]+ SHX|SHX → [\d,.]+ XLM/).first()
  ).toBeVisible();
});

test('classic pool detail keeps its classic sections', async ({ page }) => {
  await page.goto('/liquidity-pools?kind=classic');
  const firstId = page.locator('a[href^="/liquidity-pools/L"]').first();
  await expect(firstId).toBeVisible();
  await firstId.click();
  await expect(page).toHaveURL(/\/liquidity-pools\/L/);
  await expect(page.getByText('Pool participants')).toBeVisible();
});
