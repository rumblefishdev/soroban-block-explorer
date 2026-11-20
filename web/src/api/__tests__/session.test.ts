import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createSession } from '../session.js';

// The session lives in an HttpOnly cookie the API sets (task 0677): the SPA
// never sees it, so `renew` only has to make the API set a fresh one.
function minted() {
  return new Response(null, { status: 204 });
}

describe('createSession', () => {
  const fetchMock = vi.fn<typeof fetch>();
  const solve = vi.fn(async () => 'turnstile-token');

  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal('fetch', fetchMock);
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    fetchMock.mockReset();
    solve.mockClear();
    document.body.innerHTML = '';
  });

  it('solves once and asks the API to set the cookie, sending credentials', async () => {
    fetchMock.mockResolvedValueOnce(minted());
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    expect(await session.renew(0)).toBe(true);
    expect(session.mints()).toBe(1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('https://api/auth/session');
    expect(init?.method).toBe('POST');
    // Without it the browser drops the cross-origin `Set-Cookie`.
    expect(init?.credentials).toBe('include');
    expect(JSON.parse(init?.body as string)).toEqual({
      token: 'turnstile-token',
    });
  });

  it('shares one check between concurrent callers', async () => {
    fetchMock.mockResolvedValueOnce(minted());
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    expect(await Promise.all([session.renew(0), session.renew(0)])).toEqual([
      true,
      true,
    ]);
    expect(solve).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  // Decision 12: a read sent before the last mint finished only needs to be
  // repeated; its cookie-less 401 is not a new expiry.
  it('does not check again for a read sent before the last mint finished', async () => {
    fetchMock.mockResolvedValueOnce(minted());
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    await session.renew(0);
    expect(await session.renew(0)).toBe(true);
    expect(solve).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('checks again for a read sent after the last mint (an expired session)', async () => {
    fetchMock.mockResolvedValueOnce(minted()).mockResolvedValueOnce(minted());
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    await session.renew(0);
    expect(await session.renew(session.mints())).toBe(true);
    expect(session.mints()).toBe(2);
  });

  it('treats a 404 from the mint like any refusal: the retry panel', async () => {
    fetchMock.mockResolvedValueOnce(new Response('', { status: 404 }));
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
    });

    let settled = false;
    void session.renew(0).then(() => {
      settled = true;
    });
    await vi.advanceTimersByTimeAsync(0);

    expect(settled).toBe(false);
    expect(document.body.textContent).toContain('Try again');
    expect(console.warn).toHaveBeenCalledWith(expect.stringContaining('404'));
  });

  // Decision 14 (W46 B): only a browser that refuses a test cookie is told it
  // blocks cookies; it gets one panel and no further checks.
  it('shows the cookies panel once and stops checking when a test cookie is refused', async () => {
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
      keepsCookies: () => false,
    });

    expect(
      await Promise.all([session.sessionLost(), session.sessionLost()])
    ).toEqual([false, false]);

    expect(document.body.textContent).toBe('This site needs cookies');
    expect(document.querySelectorAll('[role="alert"]')).toHaveLength(1);
    expect(await session.renew(session.mints())).toBe(false);
    expect(solve).not.toHaveBeenCalled();
  });

  // A browser that keeps cookies lost the session on our side: the visitor
  // is not blamed, and "Try again" allows a fresh check.
  it('shows Try again, not the cookies panel, when the browser keeps a test cookie', async () => {
    fetchMock.mockResolvedValueOnce(minted());
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
      keepsCookies: () => true,
    });

    const lost = session.sessionLost();
    await vi.advanceTimersByTimeAsync(0);
    expect(document.body.textContent).toContain('Try again');
    expect(document.body.textContent).not.toContain('needs cookies');

    document.querySelector('button')?.click();
    expect(await lost).toBe(true);
    expect(await session.renew(session.mints())).toBe(true);
    expect(session.mints()).toBe(1);
  });

  // While the "Try again" card waits, nothing is checked: a read that gets a
  // 401 meanwhile waits for the click too (code review of W46, F1).
  it('starts no check while the Try again card waits for its click', async () => {
    fetchMock.mockResolvedValueOnce(minted());
    const session = createSession({
      siteKey: 'key',
      apiBaseUrl: 'https://api',
      solve,
      keepsCookies: () => true,
    });

    const lost = session.sessionLost();
    const meanwhile = session.renew(session.mints());
    await vi.advanceTimersByTimeAsync(0);
    expect(solve).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();

    document.querySelector('button')?.click();
    expect(await lost).toBe(true);
    expect(await meanwhile).toBe(true);
    expect(solve).toHaveBeenCalledTimes(1);
  });

  it('keeps state per instance', async () => {
    fetchMock.mockResolvedValueOnce(minted());
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

    await a.renew(0);
    expect(a.mints()).toBe(1);
    expect(b.mints()).toBe(0);
  });
});
