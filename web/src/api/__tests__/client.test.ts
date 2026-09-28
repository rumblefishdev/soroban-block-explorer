import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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

type Call = { url: string; method: string; auth: string | null; body: string };

function stubFetch(dataStatuses: number[]) {
  const calls: Call[] = [];
  let mints = 0;
  const fetchMock = vi.fn(
    async (input: RequestInfo | URL, init?: RequestInit) => {
      const req = input instanceof Request ? input : new Request(input, init);
      const call = {
        url: req.url,
        method: req.method,
        auth: req.headers.get('Authorization'),
        body: req.method === 'POST' ? await req.clone().text() : '',
      };
      calls.push(call);
      if (new URL(req.url).pathname === '/auth/session') {
        mints += 1;
        return new Response(
          JSON.stringify({ token: `jwt${mints}`, expires_in: 900 }),
          { status: 200, headers: { 'Content-Type': 'application/json' } }
        );
      }
      const status = dataStatuses.shift() ?? 200;
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
  return import('@rumblefish/api-types');
}

describe('client.ts session wiring', () => {
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

  it('sends no Authorization and mints nothing when the layer is dark', async () => {
    siteKey = undefined;
    const calls = stubFetch([]);
    const { getNetworkStats } = await freshClient();

    await getNetworkStats();

    expect(calls.map((c) => new URL(c.url).pathname)).toEqual([
      '/v1/network/stats',
    ]);
    expect(calls[0].auth).toBeNull();
  });

  it('mints once for parallel calls and sends the JWT on each', async () => {
    siteKey = 'key';
    const calls = stubFetch([]);
    const { getNetworkStats } = await freshClient();

    await Promise.all([getNetworkStats(), getNetworkStats()]);

    const mints = calls.filter((c) => c.url === `${BASE}/auth/session`);
    expect(mints).toHaveLength(1);
    expect(mints[0].method).toBe('POST');
    expect(JSON.parse(mints[0].body)).toEqual({ token: 'ts' });
    expect(mints[0].auth).toBeNull();
    const data = calls.filter((c) => c.url.endsWith('/v1/network/stats'));
    expect(data.map((c) => c.auth)).toEqual(['Bearer jwt1', 'Bearer jwt1']);
  });

  it('drops the JWT after a 401 and mints a fresh one for the next call', async () => {
    siteKey = 'key';
    const calls = stubFetch([401, 200]);
    const { getNetworkStats } = await freshClient();

    await getNetworkStats();
    await getNetworkStats();

    const data = calls.filter((c) => c.url.endsWith('/v1/network/stats'));
    expect(data.map((c) => c.auth)).toEqual(['Bearer jwt1', 'Bearer jwt2']);
  });
});
