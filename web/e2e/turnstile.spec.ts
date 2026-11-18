import { expect, test, type Page } from '@playwright/test';

/**
 * The browser check against the REAL Cloudflare Turnstile widget (task 0289).
 *
 * Each describe block talks to its own dev server, built with one of
 * Cloudflare's dummy site keys (see `playwright.config.ts`): one that always
 * passes, two that always fail, one that always asks for a challenge. The
 * widget and its script come from challenges.cloudflare.com, so these tests
 * need the internet. The API itself is never reached: the session mint and
 * every data call are answered by `page.route`.
 */

const PASSES = 'http://localhost:4281';
const FAILS = 'http://localhost:4282';
const CHALLENGES = 'http://localhost:4283';
const FAILS_INVISIBLE = 'http://localhost:4284';

const JWT = 'e2e-session-jwt';

/** A refusal shows the card in seconds; a stall shows it after the 20 s watchdog. */
const CARD_TIMEOUT = 25_000;

const ACCOUNT = 'GDXWIA4VF3GW2R5OSVIROD47W6AQHE33DSEG6TF7YZD3DYOVU54MYBEN';
const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

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

const ACCOUNT_BODY = {
  account_id: ACCOUNT,
  sequence_number: 251_123_774_269_685,
  balances: [asset('KALE', '11010000'), asset('AQUA', '0'), asset('SHX', '0')],
  home_domain: null,
  first_seen_ledger: 58_469_310,
  last_seen_ledger: 62_814_867,
  deleted: false,
  signing: null,
};

type Seen = {
  mints: { token: unknown }[];
  data: { url: string; authorization: string | undefined }[];
  /** 'mint' and 'data' in the order the browser sent them. */
  order: string[];
};

/**
 * Answer the API: the session mint with a fixed JWT, the account with a body
 * the page can render, every other data call with an empty list. Records
 * every call so the tests can say what went out and in which order.
 */
async function serveApi(page: Page): Promise<Seen> {
  const seen: Seen = { mints: [], data: [], order: [] };

  await page.route('**/api/auth/session', async (route) => {
    seen.mints.push(route.request().postDataJSON());
    seen.order.push('mint');
    await route.fulfill({ json: { token: JWT, expires_in: 900 } });
  });

  await page.route('**/api/v1/**', async (route) => {
    const request = route.request();
    seen.data.push({
      url: request.url(),
      authorization: request.headers()['authorization'],
    });
    seen.order.push('data');
    if (new URL(request.url()).pathname === `/api/v1/accounts/${ACCOUNT}`) {
      await route.fulfill({ json: ACCOUNT_BODY });
      return;
    }
    await route.fulfill({
      json: {
        data: [],
        page: { next_cursor: null, prev_cursor: null, limit: 20 },
      },
    });
  });

  return seen;
}

function failureCard(page: Page) {
  return page.getByRole('alertdialog');
}

test.describe('a key that always passes', () => {
  test.use({ baseURL: PASSES });
  test.setTimeout(90_000);

  test('mints a session with the widget token, then loads data with the Bearer', async ({
    page,
  }) => {
    const seen = await serveApi(page);
    await page.goto(`/accounts/${ACCOUNT}`);

    await expect(page.getByText('3 assets · 1 with a balance')).toBeVisible({
      timeout: CARD_TIMEOUT,
    });

    expect(seen.mints.length).toBeGreaterThanOrEqual(1);
    // Cloudflare's dummy keys hand out this exact token: proof that the real
    // widget ran and its token, not a placeholder, went to the mint.
    expect(seen.mints[0].token).toBe('XXXX.DUMMY.TOKEN.XXXX');

    // No data call may go out before a session exists.
    expect(seen.order[0]).toBe('mint');
    expect(seen.data.length).toBeGreaterThan(0);
    for (const call of seen.data) {
      expect(call.authorization, call.url).toBe(`Bearer ${JWT}`);
    }
  });

  test('one solve serves the next page too', async ({ page }) => {
    // Task AC: "data loads with at most one solve per session".
    const seen = await serveApi(page);
    await page.goto(`/accounts/${ACCOUNT}`);
    await expect(page.getByText('3 assets · 1 with a balance')).toBeVisible({
      timeout: CARD_TIMEOUT,
    });
    const dataBefore = seen.data.length;

    // Move inside the SPA to a page that asks for new data.
    await page
      .locator('.MuiCard-root')
      .filter({ hasText: /^Assets/ })
      .getByRole('link')
      .first()
      .click();
    await expect(page).not.toHaveURL(/accounts/);
    await expect.poll(() => seen.data.length).toBeGreaterThan(dataBefore);

    expect(seen.mints).toHaveLength(1);
    for (const call of seen.data) {
      expect(call.authorization, call.url).toBe(`Bearer ${JWT}`);
    }
  });
});

test.describe('a key that always fails', () => {
  test.use({ baseURL: FAILS });
  test.setTimeout(150_000);

  test('shows the plain failure card and sends no data request', async ({
    page,
  }) => {
    const seen = await serveApi(page);
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    await expect(card).toContainText("We couldn't verify your browser");
    await expect(card).toContainText(
      'This explorer checks that visitors are human before it loads data.'
    );
    await expect(card.getByRole('button', { name: 'Try again' })).toBeVisible();

    await expect(page.getByText('Something went wrong')).toHaveCount(0);
    expect(seen.data).toHaveLength(0);

    // No HTTP status (401, 403, …) and no Cloudflare error code (300030,
    // 110200, 600010, …) in what the visitor reads.
    const text = await card.innerText();
    expect(text).not.toMatch(/\b\d{3}\b|\b\d{6}\b|error code/i);
  });

  test('the card is an alertdialog with focus on Try again', async ({
    page,
  }) => {
    await serveApi(page);
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    await expect(card.getByRole('button', { name: 'Try again' })).toBeFocused();
  });

  test('Try again checks again without reloading the page', async ({
    page,
  }) => {
    const seen = await serveApi(page);
    let documentLoads = 0;
    page.on('framenavigated', (frame) => {
      if (frame === page.mainFrame()) documentLoads += 1;
    });
    // Each check draws a new widget, and each widget loads its own iframe
    // from Cloudflare's challenge platform.
    let widgetLoads = 0;
    page.on('request', (request) => {
      if (
        request.frame() !== page.mainFrame() &&
        request
          .url()
          .startsWith(
            'https://challenges.cloudflare.com/cdn-cgi/challenge-platform/'
          ) &&
        request.resourceType() === 'document'
      ) {
        widgetLoads += 1;
      }
    });
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    const url = page.url();
    const loadsBefore = documentLoads;
    const widgetsBefore = widgetLoads;

    // App state a reload would wipe, and a watcher that notes the card going
    // away even if it comes back between two polls.
    await page.evaluate(() => {
      const w = window as unknown as Record<string, unknown>;
      w.__e2eMarker = 'kept';
      w.__e2eCardGone = false;
      new MutationObserver(() => {
        if (!document.querySelector('[role="alertdialog"]')) {
          w.__e2eCardGone = true;
        }
      }).observe(document.body, {
        childList: true,
        subtree: true,
        attributes: true,
      });
    });

    await card.getByRole('button', { name: 'Try again' }).click();

    await expect
      .poll(() =>
        page.evaluate(
          () => (window as unknown as Record<string, unknown>).__e2eCardGone
        )
      )
      .toBe(true);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    expect(widgetLoads).toBeGreaterThan(widgetsBefore);

    expect(page.url()).toBe(url);
    expect(documentLoads).toBe(loadsBefore);
    expect(
      await page.evaluate(
        () => (window as unknown as Record<string, unknown>).__e2eMarker
      )
    ).toBe('kept');
    expect(seen.data).toHaveLength(0);
  });

  test.describe('on a phone', () => {
    test.use({ viewport: { width: 375, height: 812 } });

    test('the card spans the screen less 16 px a side, fully on screen', async ({
      page,
    }) => {
      await serveApi(page);
      await page.goto(`/accounts/${ACCOUNT}`);

      const card = failureCard(page);
      await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
      const box = await card.boundingBox();
      expect(box).not.toBeNull();
      if (!box) return;
      expect(box.width).toBeGreaterThanOrEqual(341);
      expect(box.width).toBeLessThanOrEqual(345);
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(375);
      expect(box.y + box.height).toBeLessThanOrEqual(812);
    });
  });

  test('on a desktop the card is at most 360 px wide', async ({ page }) => {
    await serveApi(page);
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    const box = await card.boundingBox();
    expect(box).not.toBeNull();
    expect(box?.width ?? Infinity).toBeLessThanOrEqual(360);
  });
});

test.describe('an invisible key that always fails', () => {
  test.use({ baseURL: FAILS_INVISIBLE });
  test.setTimeout(90_000);

  test('shows the same failure card and sends no data request', async ({
    page,
  }) => {
    const seen = await serveApi(page);
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    await expect(card).toContainText("We couldn't verify your browser");
    await expect(card.getByRole('button', { name: 'Try again' })).toBeVisible();
    await expect(page.getByText('Something went wrong')).toHaveCount(0);
    expect(seen.data).toHaveLength(0);
  });
});

test.describe('a key that forces an interactive challenge', () => {
  test.use({ baseURL: CHALLENGES });
  test.setTimeout(90_000);

  test('asks the visitor to confirm, keeps the widget up, and waits', async ({
    page,
  }) => {
    const seen = await serveApi(page);
    await page.goto(`/accounts/${ACCOUNT}`);

    await expect(
      page.getByText("Confirm you're human to keep loading data")
    ).toBeVisible({ timeout: CARD_TIMEOUT });

    // The widget's iframe sits in Turnstile's shadow root, so find it through
    // the page's frames rather than a selector.
    async function widgetOnScreen(): Promise<boolean> {
      const frame = page
        .frames()
        .find((f) => f.url().includes('challenges.cloudflare.com'));
      if (!frame) return false;
      const element = await frame.frameElement();
      const box = await element.boundingBox();
      const viewport = page.viewportSize();
      if (!box || !viewport) return false;
      return (
        (await element.isVisible()) &&
        box.width > 0 &&
        box.height > 0 &&
        box.x >= 0 &&
        box.y >= 0 &&
        box.x + box.width <= viewport.width &&
        box.y + box.height <= viewport.height
      );
    }

    await expect.poll(widgetOnScreen, { timeout: 15_000 }).toBe(true);

    await page.waitForTimeout(25_000);

    expect(await widgetOnScreen()).toBe(true);
    await expect(
      page.getByText("Confirm you're human to keep loading data")
    ).toBeVisible();
    await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(
      0
    );
    expect(seen.data).toHaveLength(0);
  });

  test.describe('on a phone', () => {
    test.use({ viewport: { width: 375, height: 812 } });

    test("shows Cloudflare's widget at its full width, on screen", async ({
      page,
    }) => {
      await serveApi(page);
      await page.goto(`/accounts/${ACCOUNT}`);
      await expect(
        page.getByText("Confirm you're human to keep loading data")
      ).toBeVisible({ timeout: CARD_TIMEOUT });

      // A card half the screen wide squeezed the widget to ~140 px and cut
      // its label; the widget asks for 300 px.
      async function widgetBox() {
        const frame = page
          .frames()
          .find((f) => f.url().includes('challenges.cloudflare.com'));
        if (!frame) return null;
        return (await frame.frameElement()).boundingBox();
      }
      await expect
        .poll(async () => (await widgetBox())?.width ?? 0, { timeout: 15_000 })
        .toBeGreaterThanOrEqual(290);
      const box = await widgetBox();
      expect(box).not.toBeNull();
      if (!box) return;
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(375);
    });
  });
});
