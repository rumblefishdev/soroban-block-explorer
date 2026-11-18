import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createSession } from '../session.js';

// Drives the real Turnstile solver and retry panel in `session.ts` through a
// fake `window.turnstile`: the widget's callbacks are fired by hand, the way
// Cloudflare fires them.

type RenderOptions = Record<string, unknown> & {
  callback: (token: string) => void;
  'error-callback': (code: string) => void;
  'before-interactive-callback': () => void;
};

function fakeTurnstile() {
  const renders: RenderOptions[] = [];
  const api = {
    render: vi.fn((_container: HTMLElement, opts: RenderOptions) => {
      renders.push(opts);
      return `widget-${renders.length}`;
    }),
    remove: vi.fn(),
  };
  return { api, renders };
}

function okResponse(token: string) {
  return new Response(JSON.stringify({ token, expires_in: 900 }), {
    status: 200,
  });
}

function retryButton() {
  return Array.from(document.querySelectorAll('button')).find(
    (b) => b.textContent === 'Try again'
  );
}

describe('Turnstile solver and retry panel', () => {
  const fetchMock = vi.fn<typeof fetch>();

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
    delete window.turnstile;
    document.body.innerHTML = '';
    document.head.innerHTML = '';
  });

  function armedSession() {
    return createSession({ siteKey: 'key', apiBaseUrl: 'https://api' });
  }

  it('renders a widget Cloudflare may not restart on its own', async () => {
    const { api, renders } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    void armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(0);

    expect(renders[0]).toMatchObject({
      appearance: 'interaction-only',
      'refresh-timeout': 'manual',
      retry: 'never',
    });
  });

  it('keeps an interactive challenge on screen, past the watchdog, until it is solved', async () => {
    const { api, renders } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    fetchMock.mockResolvedValueOnce(okResponse('jwt-1'));
    const pending = armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(0);

    renders[0]['before-interactive-callback']();
    // Cloudflare enters interactive mode again after a refresh in the widget.
    renders[0]['before-interactive-callback']();
    expect(
      document.body.textContent?.split("Confirm you're human").length
    ).toBe(2);

    await vi.advanceTimersByTimeAsync(5 * 60_000);
    expect(api.remove).not.toHaveBeenCalled();
    expect(retryButton()).toBeUndefined();

    renders[0].callback('turnstile-token');
    expect(await pending).toBe('jwt-1');
    expect(api.remove).toHaveBeenCalledWith('widget-1');
    expect(document.body.textContent).not.toContain('Confirm');
  });

  it('shows the retry panel at once after a server error, and retries on click', async () => {
    const { api, renders } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    fetchMock
      .mockResolvedValueOnce(new Response('busy', { status: 503 }))
      .mockResolvedValueOnce(okResponse('jwt-1'));
    const pending = armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(0);

    renders[0].callback('turnstile-token');
    await vi.advanceTimersByTimeAsync(0);
    expect(retryButton()).toBeDefined();
    expect(api.render).toHaveBeenCalledTimes(1);

    retryButton()?.click();
    await vi.advanceTimersByTimeAsync(0);
    renders[1].callback('turnstile-token-2');
    expect(await pending).toBe('jwt-1');
  });

  it('turns a 20 s stall into the retry panel, and retries on click', async () => {
    const { api, renders } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    fetchMock.mockResolvedValueOnce(okResponse('jwt-1'));
    const pending = armedSession().ensureToken();

    await vi.advanceTimersByTimeAsync(20_000);
    expect(api.remove).toHaveBeenCalledWith('widget-1');
    expect(document.body.textContent).toContain(
      "We couldn't verify your browser"
    );
    expect(api.render).toHaveBeenCalledTimes(1);

    retryButton()?.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(api.render).toHaveBeenCalledTimes(2);
    expect(retryButton()).toBeUndefined();

    renders[1].callback('turnstile-token');
    expect(await pending).toBe('jwt-1');
  });

  it('labels the panel for assistive technology and focuses its button', async () => {
    const { api } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    void armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(20_000);

    const panel = document.querySelector('[role="alertdialog"]');
    const titleId = panel?.getAttribute('aria-labelledby') ?? '';
    expect(document.getElementById(titleId)?.textContent).toBe(
      "We couldn't verify your browser"
    );
    expect(document.activeElement).toBe(retryButton());
  });

  it('fills a phone screen less a margin, up to 360 px', async () => {
    const { api } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    void armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(20_000);

    const panel = document.querySelector<HTMLElement>('[role="alertdialog"]');
    expect(panel?.style.width).toBe('calc(100vw - 32px)');
    expect(panel?.style.boxSizing).toBe('border-box');
    expect(panel?.style.maxWidth).toBe('360px');
  });

  it("sizes the click card to Cloudflare's widget, not to the phone", async () => {
    const { api, renders } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    void armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(0);

    renders[0]['before-interactive-callback']();
    const card = api.render.mock.calls[0][0];
    expect(card.style.maxWidth).toBe('360px');
    expect(card.style.width).toBe('');
  });

  it("keeps Cloudflare's error code out of the panel, in the console", async () => {
    const { api, renders } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    void armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(0);

    renders[0]['error-callback']('600010');
    await vi.advanceTimersByTimeAsync(0);

    expect(retryButton()).toBeDefined();
    expect(document.body.textContent).not.toContain('600010');
    expect(console.warn).toHaveBeenCalledWith(
      expect.stringContaining('600010')
    );
  });

  it('keeps the HTTP status out of the panel, in the console, when the API refuses the token', async () => {
    const { api, renders } = fakeTurnstile();
    window.turnstile = api as unknown as Window['turnstile'];
    fetchMock.mockResolvedValue(new Response('nope', { status: 403 }));
    void armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(0);

    renders[0].callback('turnstile-token');
    await vi.advanceTimersByTimeAsync(0);

    expect(retryButton()).toBeDefined();
    expect(document.body.textContent).not.toContain('403');
    expect(console.warn).toHaveBeenCalledWith(expect.stringContaining('403'));
  });

  it('loads the script again on retry when it came up without window.turnstile', async () => {
    void armedSession().ensureToken();
    await vi.advanceTimersByTimeAsync(0);
    const scripts = () => document.head.querySelectorAll('script');
    expect(scripts()).toHaveLength(1);

    scripts()[0].dispatchEvent(new Event('load'));
    await vi.advanceTimersByTimeAsync(0);
    expect(retryButton()).toBeDefined();

    retryButton()?.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(scripts()).toHaveLength(2);
  });
});
