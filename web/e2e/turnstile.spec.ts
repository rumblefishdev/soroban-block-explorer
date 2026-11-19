import { expect, test, type BrowserContext, type Page } from '@playwright/test';

/**
 * The browser check against the REAL Cloudflare Turnstile widget (task 0289).
 *
 * Each describe block talks to its own dev server, built with one of
 * Cloudflare's dummy site keys (see `playwright.config.ts`): one that always
 * passes, two that always fail, one that always asks for a challenge. The
 * widget and its script come from challenges.cloudflare.com, so these tests
 * need the internet. The API itself is never reached: the session mint and
 * every data call are answered by `page.route`. The mint answers with the
 * same `Set-Cookie` the real API sends (task 0677), and a data call passes
 * only when the browser sends that cookie back — so these tests prove the
 * browser keeps it across reloads and pages.
 */

const PASSES = 'http://localhost:4281';
const FAILS = 'http://localhost:4282';
const CHALLENGES = 'http://localhost:4283';
const FAILS_INVISIBLE = 'http://localhost:4284';

/** A refusal shows the card within seconds. */
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
  data: {
    url: string;
    authorization: string | undefined;
    status: number;
  }[];
  /** 'mint' and 'data' in the order the browser sent them. */
  order: string[];
};

const SESSION_COOKIE = '__Host-session=e2e-session';

/**
 * Answer the API: the session mint with 204 and the session cookie, the
 * account with a body the page can render, every other data call with an
 * empty list — and every data call that does not send the cookie with 401.
 * With `cookieSticks: false` the mint sets no cookie, which is what a browser
 * that keeps no cookies sees. Records every call so the tests can say what
 * went out and in which order. Routes the whole context, so a second page
 * shares the counts.
 */
async function serveApi(
  context: BrowserContext,
  { cookieSticks = true } = {}
): Promise<Seen> {
  const seen: Seen = { mints: [], data: [], order: [] };

  await context.route('**/api/auth/session', async (route) => {
    seen.mints.push(route.request().postDataJSON());
    seen.order.push('mint');
    await route.fulfill({
      status: 204,
      headers: cookieSticks
        ? {
            'set-cookie': `${SESSION_COOKIE}; Max-Age=900; Path=/; Secure; HttpOnly; SameSite=Strict`,
          }
        : {},
    });
  });

  await context.route('**/api/v1/**', async (route) => {
    const request = route.request();
    const cookie = (await request.allHeaders())['cookie'] ?? '';
    const status = cookie.includes(SESSION_COOKIE) ? 200 : 401;
    seen.data.push({
      url: request.url(),
      authorization: request.headers()['authorization'],
      status,
    });
    seen.order.push('data');
    if (status === 401) {
      await route.fulfill({ status, body: 'authentication required' });
      return;
    }
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

  test('answers the first 401 with a check and a mint, then loads data', async ({
    page,
  }) => {
    const seen = await serveApi(page.context());
    await page.goto(`/accounts/${ACCOUNT}`);

    await expect(page.getByText('3 assets · 1 with a balance')).toBeVisible({
      timeout: CARD_TIMEOUT,
    });

    expect(seen.mints.length).toBeGreaterThanOrEqual(1);
    // Cloudflare's dummy keys hand out this exact token: proof that the real
    // widget ran and its token, not a placeholder, went to the mint.
    expect(seen.mints[0].token).toBe('XXXX.DUMMY.TOKEN.XXXX');

    // Reads go out at once, and their 401s share one check (decision 5).
    expect(seen.order[0]).toBe('data');
    expect(seen.mints).toHaveLength(1);
    expect(seen.data.some((call) => call.status === 200)).toBe(true);
    // The session is the cookie, never a header (decision 6).
    for (const call of seen.data) {
      expect(call.authorization, call.url).toBeUndefined();
    }
  });

  // Decision 14 (W46 B): the browser keeps cookies but the session cookie
  // never comes back (here the mint sets none, as a proxy stripping it would):
  // the visitor is not blamed, the retry card shows, nothing is checked again
  // before the click.
  test('a session lost on our side shows Try again, not the cookies panel', async ({
    page,
  }) => {
    const seen = await serveApi(page.context(), { cookieSticks: false });
    await page.goto(`/accounts/${ACCOUNT}`);

    await expect(failureCard(page)).toContainText(
      "We couldn't verify your browser",
      { timeout: CARD_TIMEOUT }
    );
    await expect(page.getByText('This site needs cookies')).toHaveCount(0);
    expect(seen.mints).toHaveLength(1);
  });

  test('one solve serves the next page too', async ({ page }) => {
    // Task AC: "data loads with at most one solve per session".
    const seen = await serveApi(page.context());
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
  });

  // Decision 2, the goal of task 0677: the browser keeps the cookie, so a
  // reload and a second tab read data with no new check.
  test('a reload and a second tab need no new check', async ({ page }) => {
    const seen = await serveApi(page.context());
    await page.goto(`/accounts/${ACCOUNT}`);
    await expect(page.getByText('3 assets · 1 with a balance')).toBeVisible({
      timeout: CARD_TIMEOUT,
    });

    await page.reload();
    await expect(page.getByText('3 assets · 1 with a balance')).toBeVisible({
      timeout: CARD_TIMEOUT,
    });

    const second = await page.context().newPage();
    await second.goto(`/accounts/${ACCOUNT}`);
    await expect(second.getByText('3 assets · 1 with a balance')).toBeVisible({
      timeout: CARD_TIMEOUT,
    });

    expect(seen.mints).toHaveLength(1);
  });
});

test.describe('a key that always fails', () => {
  test.use({ baseURL: FAILS });
  test.setTimeout(150_000);

  test('shows the plain failure card and no data loads', async ({ page }) => {
    const seen = await serveApi(page.context());
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    await expect(card).toContainText("We couldn't verify your browser");
    await expect(card).not.toContainText('This explorer checks');
    await expect(card.getByRole('button', { name: 'Try again' })).toBeVisible();

    await expect(page.getByText('Something went wrong')).toHaveCount(0);
    // Reads go out at once (decision 5), but none gets past its 401.
    expect(seen.data.every((call) => call.status === 401)).toBe(true);

    // No HTTP status (401, 403, …) and no Cloudflare error code (300030,
    // 110200, 600010, …) in what the visitor reads.
    const text = await card.innerText();
    expect(text).not.toMatch(/\b\d{3}\b|\b\d{6}\b|error code/i);
  });

  test('the card is an alertdialog with focus on Try again', async ({
    page,
  }) => {
    await serveApi(page.context());
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    await expect(card.getByRole('button', { name: 'Try again' })).toBeFocused();
  });

  test('Try again checks again without reloading the page', async ({
    page,
  }) => {
    const seen = await serveApi(page.context());
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
    // Reads go out at once (decision 5), but none gets past its 401.
    expect(seen.data.every((call) => call.status === 401)).toBe(true);
  });

  test.describe('on a phone', () => {
    test.use({ viewport: { width: 375, height: 812 } });

    test('the card spans the screen less 16 px a side, fully on screen', async ({
      page,
    }) => {
      await serveApi(page.context());
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
    await serveApi(page.context());
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

  test('shows the same failure card and no data loads', async ({ page }) => {
    const seen = await serveApi(page.context());
    await page.goto(`/accounts/${ACCOUNT}`);

    const card = failureCard(page);
    await expect(card).toBeVisible({ timeout: CARD_TIMEOUT });
    await expect(card).toContainText("We couldn't verify your browser");
    await expect(card.getByRole('button', { name: 'Try again' })).toBeVisible();
    await expect(page.getByText('Something went wrong')).toHaveCount(0);
    // Reads go out at once (decision 5), but none gets past its 401.
    expect(seen.data.every((call) => call.status === 401)).toBe(true);
  });
});

test.describe('a key that forces an interactive challenge', () => {
  test.use({ baseURL: CHALLENGES });
  test.setTimeout(90_000);

  test("shows Cloudflare's widget alone, keeps it up, and waits", async ({
    page,
  }) => {
    const seen = await serveApi(page.context());
    await page.goto(`/accounts/${ACCOUNT}`);

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

    await expect.poll(widgetOnScreen, { timeout: CARD_TIMEOUT }).toBe(true);

    await page.waitForTimeout(25_000);

    expect(await widgetOnScreen()).toBe(true);
    await expect(page.getByText("Confirm you're human")).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(
      0
    );
    // Reads go out at once (decision 5), but none gets past its 401.
    expect(seen.data.every((call) => call.status === 401)).toBe(true);
  });

  test.describe('on a phone', () => {
    test.use({ viewport: { width: 375, height: 812 } });

    test("shows Cloudflare's widget at its full width, on screen", async ({
      page,
    }) => {
      await serveApi(page.context());
      await page.goto(`/accounts/${ACCOUNT}`);

      // A container half the screen wide squeezed the widget to ~140 px and cut
      // its label; the widget asks for 300 px.
      async function widgetBox() {
        const frame = page
          .frames()
          .find((f) => f.url().includes('challenges.cloudflare.com'));
        if (!frame) return null;
        return (await frame.frameElement()).boundingBox();
      }
      await expect
        .poll(async () => (await widgetBox())?.width ?? 0, {
          timeout: CARD_TIMEOUT,
        })
        .toBeGreaterThanOrEqual(290);
      const box = await widgetBox();
      expect(box).not.toBeNull();
      if (!box) return;
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(375);
    });
  });
});
