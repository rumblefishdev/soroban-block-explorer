import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createSession, isSessionMint } from '../session.js';

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
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
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

  it('waits through a failed solve and a refused exchange, then resolves the JWT', async () => {
    solve.mockRejectedValueOnce(new Error('widget failed'));
    fetchMock
      .mockResolvedValueOnce(new Response('nope', { status: 403 }))
      .mockResolvedValueOnce(okResponse('jwt-after-failure'));
    const waitForRetry = vi.fn<(reason: string) => Promise<void>>(
      async () => undefined
    );
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
      waitForRetry,
    });

    // Never `null` while armed: a request must not go out without a session.
    expect(await session.ensureToken()).toBe('jwt-after-failure');
    expect(solve).toHaveBeenCalledTimes(3);
    expect(waitForRetry).toHaveBeenCalledTimes(2);
    expect(waitForRetry.mock.calls[0][0]).toContain('widget failed');
    expect(waitForRetry.mock.calls[1][0]).toContain('403');
  });

  it('does not solve again until the visitor asks to retry', async () => {
    solve.mockRejectedValueOnce(new Error('widget failed'));
    fetchMock.mockResolvedValueOnce(okResponse('jwt-1'));
    let retry: () => void = () => undefined;
    const waitForRetry = vi.fn<(reason: string) => Promise<void>>(
      () => new Promise<void>((resolve) => (retry = resolve))
    );
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
      waitForRetry,
    });

    const pending = session.ensureToken();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(solve).toHaveBeenCalledTimes(1);
    expect(fetchMock).not.toHaveBeenCalled();

    retry();
    expect(await pending).toBe('jwt-1');
    expect(solve).toHaveBeenCalledTimes(2);
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

describe('isSessionMint', () => {
  it('matches the mint path under any base URL, and nothing else', () => {
    expect(isSessionMint('https://api.example/auth/session')).toBe(true);
    expect(isSessionMint('https://api.example/prefix/auth/session')).toBe(true);
    expect(isSessionMint('https://api.example/v1/accounts')).toBe(false);
    expect(isSessionMint('https://api.example/v1/search?q=/auth/session')).toBe(
      false
    );
  });
});
