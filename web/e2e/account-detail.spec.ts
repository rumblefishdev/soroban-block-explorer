import { expect, test, type Page } from '@playwright/test';

/**
 * Issue #377 in a real browser.
 *
 * These assertions are deliberately the ones jsdom cannot make. Everything
 * about WHAT the page renders is already covered by the component tests —
 * chips, ordering, the four signer states, the caption. What only a browser
 * can answer is whether the history entry survives a round trip, and whether
 * a thousand rows still fit in a card instead of turning the page into a
 * scroll bar.
 */

const ACCOUNT = 'GDXWIA4VF3GW2R5OSVIROD47W6AQHE33DSEG6TF7YZD3DYOVU54MYBEN';

const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';
/**
 * The Assets card. Anchored on the card's own title rather than the word
 * anywhere in its text — the Signers card says "This account holds assets"
 * when it warns, and would match too.
 */
function assetsCard(page: Page) {
  return page.locator('.MuiCard-root').filter({ hasText: /^Assets/ });
}

/** One classic trustline, funded or standing at zero. */
function asset(code: string, balance: string) {
  return {
    asset_type_name: 'credit_alphanum4',
    type: 1,
    asset_code: code,
    asset_issuer: ISSUER,
    contract_id: null,
    name: null,
    symbol: null,
    balance,
    decimals: 7,
    last_updated_ledger: 64_000_000,
    sac_deployed: false,
  };
}

/**
 * Intercept the account endpoint. The suite never reaches production: CI holds
 * no client certificate for it, and a browser test that depends on live chain
 * data measures the network rather than the page.
 */
async function serveAccount(
  page: Page,
  body: Record<string, unknown>
): Promise<void> {
  await page.route('**/v1/accounts/*', async (route) => {
    if (route.request().url().includes('/transactions')) {
      await route.fulfill({
        json: {
          data: [],
          page: { next_cursor: null, prev_cursor: null, limit: 20 },
        },
      });
      return;
    }
    await route.fulfill({ json: body });
  });
}

function account(balances: unknown[], signing: unknown) {
  return {
    account_id: ACCOUNT,
    // Deliberately inside Number.MAX_SAFE_INTEGER. A real Stellar sequence
    // number is an int64 and can exceed it, which is a genuine wire concern —
    // but it is not what these tests measure, and a literal that loses
    // precision at parse time would fail lint for a reason unrelated to them.
    sequence_number: 251_123_774_269_685,
    balances,
    home_domain: null,
    first_seen_ledger: 58_469_310,
    last_seen_ledger: 62_814_867,
    deleted: false,
    signing,
  };
}

test('the multisig account reads as multisig, with the master key counted', async ({
  page,
}) => {
  // The whole point of #377's signer half: the ledger leaves the account's own
  // key out of the list, so a page that renders only the list says 3-of-4
  // where the chain says 3-of-5.
  await serveAccount(
    page,
    account(
      [asset('KALE', '11010000'), asset('AQUA', '0'), asset('SHX', '0')],
      {
        signers: [1, 2, 3, 4].map((n) => ({
          key: `GA${'A'.repeat(52)}${n}`,
          weight: 1,
          type: 'ed25519',
        })),
        master_weight: 1,
        threshold_low: 3,
        threshold_med: 3,
        threshold_high: 3,
        last_updated_ledger: 64_115_052,
      }
    )
  );
  await page.goto(`/accounts/${ACCOUNT}`);

  await expect(page.getByText('Multisig')).toBeVisible();
  await expect(page.getByText('master key', { exact: true })).toBeVisible();
  await expect(page.getByText(/Total weight 5/)).toBeVisible();
  // Zero-balance trustlines are rows, not omissions — the other half of #377.
  await expect(page.getByText('3 assets · 1 with a balance')).toBeVisible();
});

test('paging survives opening an asset and coming back', async ({ page }) => {
  // Only a real browser can answer this: the page number lives in the URL so
  // that Back returns you where you were, and jsdom's history is a stand-in.
  const many = Array.from({ length: 45 }, (_, i) =>
    asset(`A${String(i).padStart(3, '0')}`, '0')
  );
  await serveAccount(page, account(many, null));
  await page.goto(`/accounts/${ACCOUNT}?assets=3`);

  const card = assetsCard(page);
  await expect(card.getByText('41–45 of 45')).toBeVisible();

  await card.getByRole('link').first().click();
  await expect(page).not.toHaveURL(/accounts/);

  await page.goBack();
  await expect(page).toHaveURL(/\?assets=3$/);
  await expect(card.getByText('41–45 of 45')).toBeVisible();
});

test('a thousand rows stay inside the card', async ({ page }) => {
  // "Show all" was measured at 227,223px — about 380 screen-heights. The pager
  // exists so the card keeps a human size, which is a layout fact and so needs
  // a layout engine to check.
  const many = Array.from({ length: 1015 }, (_, i) =>
    asset(`A${String(i).padStart(4, '0')}`, i < 996 ? '10000000' : '0')
  );
  await serveAccount(page, account(many, null));
  await page.goto(`/accounts/${ACCOUNT}`);

  const card = assetsCard(page);
  await expect(card.getByText('1–20 of 1015')).toBeVisible();

  const height = await card.evaluate((el) => el.getBoundingClientRect().height);
  expect(height).toBeLessThan(2000);
});
