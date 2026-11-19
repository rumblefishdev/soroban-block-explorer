import type * as ApiTypes from '@rumblefish/api-types';
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest';

// The session is wired into the shared API client by `client.ts` at import
// time. These tests import it fresh per case, with the config mocked, and
// drive real SDK calls through a stubbed `fetch` and a fake Turnstile.

const BASE = 'https://api.test';
let siteKey: string | undefined;

vi.mock('../config.js', () => ({
  get apiBaseUrl() {
    return BASE;
  },
  get turnstileSiteKey() {
    return siteKey;
  },
}));

type Call = {
  path: string;
  method: string;
  auth: string | null;
  credentials: RequestCredentials;
  body: string;
};

// A fake API: `/auth/session` answers 204 (the real one sets the cookie);
// each data read takes the next status from `dataStatuses` (default 200),
// optionally waiting on `hold` so a test can order responses around a mint.
function stubFetch(dataStatuses: number[], hold: Promise<void>[] = []) {
  const calls: Call[] = [];
  const fetchMock = vi.fn(
    async (input: RequestInfo | URL, init?: RequestInit) => {
      const req = input instanceof Request ? input : new Request(input, init);
      const path = new URL(req.url).pathname;
      calls.push({
        path,
        method: req.method,
        auth: req.headers.get('Authorization'),
        credentials: init?.credentials ?? req.credentials,
        body: req.method === 'POST' ? await req.clone().text() : '',
      });
      if (path === '/auth/session') return new Response(null, { status: 204 });
      const status = dataStatuses.shift() ?? 200;
      await hold.shift();
      return new Response(status === 200 ? '{}' : 'nope', {
        status,
        headers: { 'Content-Type': 'application/json' },
      });
    }
  );
  vi.stubGlobal('fetch', fetchMock);
  return calls;
}

async function freshClient() {
  vi.resetModules();
  await import('../client.js');
  // Not `import('@rumblefish/api-types')`: Nx reads a dynamic import as lazy
  // loading and then forbids every static import of the lib across `web`.
  return vi.importActual<typeof ApiTypes>('@rumblefish/api-types');
}

describe('client.ts session wiring', () => {
  // The first import of `client.ts` transforms the generated SDK and the UI
  // library (`session.ts` takes its panel colours from it): ~2 s alone, past
  // 5 s when the whole suite runs at once. Paid here, under the hook's own
  // timeout, so no case's 5 s goes to it; each later import takes ~50 ms.
  beforeAll(async () => {
    await freshClient();
  });

  beforeEach(() => {
    window.turnstile = {
      render: (_el, opts) => {
        queueMicrotask(() => opts.callback('ts'));
        return 'w1';
      },
      remove: () => undefined,
    };
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    delete window.turnstile;
    siteKey = undefined;
  });

  it('mints nothing when the layer is dark, and still sends credentials', async () => {
    siteKey = undefined;
    const calls = stubFetch([]);
    const { getNetworkStats } = await freshClient();

    await getNetworkStats();

    expect(calls.map((c) => c.path)).toEqual(['/v1/network/stats']);
    expect(calls[0].credentials).toBe('include');
  });

  // Decision 5: a read goes out at once; its 401 becomes one check and one
  // repeat, so the page gets the data and never the 401.
  it('turns a 401 into one check and one repeated read', async () => {
    siteKey = 'key';
    const calls = stubFetch([401, 200]);
    const { getNetworkStats } = await freshClient();

    const result = await getNetworkStats();

    expect(result.response?.status).toBe(200);
    expect(calls.map((c) => c.path)).toEqual([
      '/v1/network/stats',
      '/auth/session',
      '/v1/network/stats',
    ]);
    const mint = calls[1];
    expect(mint.method).toBe('POST');
    expect(mint.credentials).toBe('include');
    expect(JSON.parse(mint.body)).toEqual({ token: 'ts' });
    // Decision 6: the session is the cookie, never a header.
    expect(calls.every((c) => c.auth === null)).toBe(true);
  });

  it('shares one check between parallel reads that both get a 401', async () => {
    siteKey = 'key';
    const calls = stubFetch([401, 401, 200, 200]);
    const { getNetworkStats } = await freshClient();

    const results = await Promise.all([getNetworkStats(), getNetworkStats()]);

    expect(results.map((r) => r.response?.status)).toEqual([200, 200]);
    expect(calls.filter((c) => c.path === '/auth/session')).toHaveLength(1);
  });

  // Decision 12: a slow read sent before the mint finished is only repeated.
  it('repeats a read whose 401 lands after the mint, without a second check', async () => {
    siteKey = 'key';
    let releaseSlow!: () => void;
    const slow = new Promise<void>((resolve) => {
      releaseSlow = resolve;
    });
    // Read A: 401 at once. Read B: 401 held until A's mint is done.
    const calls = stubFetch([401, 401, 200, 200], [Promise.resolve(), slow]);
    const { getNetworkStats } = await freshClient();

    const a = getNetworkStats();
    const b = getNetworkStats();
    await a;
    releaseSlow();
    await b;

    expect(calls.filter((c) => c.path === '/auth/session')).toHaveLength(1);
  });

  // The session expires later: a fresh 401 after a good read is a new check,
  // not a blocked cookie.
  it('checks again when the session expires after a successful read', async () => {
    siteKey = 'key';
    const calls = stubFetch([401, 200, 401, 200]);
    const { getNetworkStats } = await freshClient();

    await getNetworkStats();
    const later = await getNetworkStats();

    expect(later.response?.status).toBe(200);
    expect(calls.filter((c) => c.path === '/auth/session')).toHaveLength(2);
    expect(document.body.textContent).not.toContain('This site needs cookies');
  });

  // Decision 14: a repeat that still gets a 401 means the browser does not
  // keep the cookie: one panel, and no more checks for later reads.
  it('shows the cookies panel and stops checking when the repeat gets a 401', async () => {
    siteKey = 'key';
    const calls = stubFetch([401, 401, 401]);
    const { getNetworkStats } = await freshClient();

    await getNetworkStats();
    await getNetworkStats();

    expect(document.body.textContent).toContain('This site needs cookies');
    expect(calls.filter((c) => c.path === '/auth/session')).toHaveLength(1);
    document.body.innerHTML = '';
  });
});
