import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createSession } from '../session.js';

const NOW = Date.parse('2026-09-25T12:00:00.000Z');

function okResponse(token: string, expiresIn = 900) {
  return new Response(JSON.stringify({ token, expires_in: expiresIn }), {
    status: 200,
  });
}

describe('createSession', () => {
  const fetchMock = vi.fn<typeof fetch>();
  const solve = vi.fn(async () => 'turnstile-token');

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    fetchMock.mockReset();
    solve.mockClear();
  });

  it('is inert without a site key', async () => {
    const session = createSession({
      siteKey: undefined,
      apiBaseUrl: 'https://api',
      solve,
    });
    expect(await session.ensureToken()).toBeNull();
    expect(solve).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('exchanges a Turnstile token for a JWT and reuses it while fresh', async () => {
    fetchMock.mockResolvedValueOnce(okResponse('jwt-1'));
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    expect(await session.ensureToken()).toBe('jwt-1');
    expect(await session.ensureToken()).toBe('jwt-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe('https://api/auth/session');
    expect(JSON.parse(fetchMock.mock.calls[0][1]?.body as string)).toEqual({
      token: 'turnstile-token',
    });
  });

  it('shares one round-trip between concurrent callers', async () => {
    fetchMock.mockResolvedValueOnce(okResponse('jwt-1'));
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    const [a, b] = await Promise.all([
      session.ensureToken(),
      session.ensureToken(),
    ]);
    expect([a, b]).toEqual(['jwt-1', 'jwt-1']);
    expect(solve).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('re-mints once the token is inside the expiry skew', async () => {
    fetchMock
      .mockResolvedValueOnce(okResponse('jwt-1', 60))
      .mockResolvedValueOnce(okResponse('jwt-2', 60));
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    expect(await session.ensureToken()).toBe('jwt-1');
    vi.setSystemTime(NOW + 31_000); // 29 s left < 30 s skew
    expect(await session.ensureToken()).toBe('jwt-2');
  });

  it('re-mints after invalidate', async () => {
    fetchMock
      .mockResolvedValueOnce(okResponse('jwt-1'))
      .mockResolvedValueOnce(okResponse('jwt-2'));
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    expect(await session.ensureToken()).toBe('jwt-1');
    session.invalidate();
    expect(await session.ensureToken()).toBe('jwt-2');
  });

  it('resolves null, not a throw, when the exchange fails', async () => {
    fetchMock.mockResolvedValueOnce(new Response('nope', { status: 403 }));
    solve.mockRejectedValueOnce(new Error('widget failed'));
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    expect(await session.ensureToken()).toBeNull(); // solve throws
    expect(await session.ensureToken()).toBeNull(); // server 403
  });

  it('keeps state per instance', async () => {
    fetchMock
      .mockResolvedValueOnce(okResponse('jwt-a'))
      .mockResolvedValueOnce(okResponse('jwt-b'));
    const a = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });
    const b = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    expect(await a.ensureToken()).toBe('jwt-a');
    expect(await b.ensureToken()).toBe('jwt-b');
    a.invalidate();
    expect(await b.ensureToken()).toBe('jwt-b');
  });
});
