import { expect, test, type Locator } from '@playwright/test';

/**
 * Issue #735 in a real browser: on a 1325 px window the Events table gave the
 * width to the event ID and squeezed Data until its JSON broke mid-word.
 * Only a browser lays out a table, so only a browser can see it.
 *
 * The three events are those of testnet transaction 014af8f6…5c62, the one
 * the issue links, as stellar-rpc `getEvents` returns them.
 */

const HASH = '014af8f6ccee57a10bc1930f6f8fbc914354cc88ef8095a2238f80ab242a5c62';
const ACCOUNT = 'GBNMVXQGIVSK35QCSYGKFVJ53XAFYO3O3PWSRJ3MOXF57M2B2BKKBOSY';
const XLM_SAC = 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC';
const TOKEN = 'CDUNRHBWKP7OZJUKQDPBOCD3XZNKC2VJH6T56A3CYNRGA32GJKVKHRVT';

const EVENTS = [
  {
    id: '0024488821190033408-0000000004',
    event_type: 'contract',
    stage: 'before_all_txs',
    contract_id: XLM_SAC,
    topics: [
      { type: 'sym', value: 'fee' },
      { type: 'address', value: ACCOUNT },
    ],
    data: { type: 'i128', value: '81147' },
  },
  {
    id: '0024488821190053888-0000000000',
    event_type: 'contract',
    operation_index: 0,
    contract_id: TOKEN,
    topics: [
      { type: 'sym', value: 'mint' },
      { type: 'address', value: ACCOUNT },
    ],
    data: { type: 'i128', value: '77777777777777' },
  },
  {
    id: '0024488825484996608-0000000001',
    event_type: 'contract',
    stage: 'after_all_txs',
    contract_id: XLM_SAC,
    topics: [
      { type: 'sym', value: 'fee' },
      { type: 'address', value: ACCOUNT },
    ],
    data: { type: 'i128', value: '-14293' },
  },
];

/** How many lines the element's text takes on screen. */
async function renderedLines(element: Locator): Promise<number> {
  return element.evaluate((node) => {
    const range = document.createRange();
    range.selectNodeContents(node);
    const tops = new Set(
      [...range.getClientRects()]
        .filter((r) => r.width > 0)
        .map((r) => Math.round(r.top))
    );
    return tops.size;
  });
}

test('the Events table keeps Data readable at the width of the report', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1325, height: 900 });
  await page.route(`**/v1/transactions/${HASH}`, (route) =>
    route.fulfill({
      json: {
        hash: HASH,
        ledger_sequence: 5_701_848,
        application_order: 1,
        fee_charged: 66_854,
        successful: true,
        operation_count: 1,
        has_soroban: true,
        created_at: '2026-11-14T10:00:00Z',
        parse_error: false,
        source_account: ACCOUNT,
        operations: [],
        participants: [],
        soroban_events: [],
        soroban_invocations: [],
        heavy_fields_status: 'ok',
        heavy: {
          signatures: [],
          operations: [],
          contract_events: EVENTS,
          diagnostic_events: [],
        },
      },
    })
  );

  await page.goto(`/transactions/${HASH}`);
  await page.getByText('Show 3 events').click();

  const table = page.getByRole('table');
  const columns = await table.getByRole('columnheader').allTextContents();
  const rows = table.getByRole('row');

  // No key or value of Topics and Data breaks mid-word: a word split
  // across two lines is one coloured span drawn as two boxes.
  for (let row = 1; row <= EVENTS.length; row++) {
    for (const column of ['Topics', 'Data']) {
      const json = rows
        .nth(row)
        .getByRole('cell')
        .nth(columns.indexOf(column))
        .locator('pre');
      const broken = await json.evaluate((pre) =>
        [...pre.querySelectorAll('span')]
          .filter((s) => s.getClientRects().length > 1)
          .map((s) => s.textContent)
      );
      expect(broken).toEqual([]);
    }
  }

  const cells = rows.nth(1).getByRole('cell');

  // The ID stays whole — every digit on screen — and wraps only after the
  // dash, so it takes at most two lines.
  const id = cells.nth(columns.indexOf('ID'));
  await expect(id).toHaveText(EVENTS[0].id);
  expect(await renderedLines(id)).toBeLessThanOrEqual(2);

  // The ID moved last: what the event says comes before where it is filed.
  expect(columns).toEqual([
    'Type',
    'Where',
    'Contract',
    'Topics',
    'Data',
    'ID',
  ]);
});
