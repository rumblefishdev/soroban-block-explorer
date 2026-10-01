import { readFileSync } from 'fs';
import { resolve } from 'path';
import { beforeAll, describe, expect, it } from 'vitest';

// The consent wiring lives as inline scripts in index.html (lore-0589).
// Run them for real: GA must start denied, and only the HubSpot banner may
// grant it.
const html = readFileSync(resolve(__dirname, '../../index.html'), 'utf8');
const inline = [
  ...new DOMParser()
    .parseFromString(html, 'text/html')
    .querySelectorAll('script:not([src])'),
].map((s) => s.textContent ?? '');

type Consent = Record<string, string>;
type Listener = (c: { categories: Record<string, boolean> }) => void;
const w = window as unknown as {
  // `gtag()` pushes `arguments`; the blocklist is a plain object.
  dataLayer: (IArguments | Record<string, unknown>)[];
  _hsp: [string, Listener][];
};

const consentCalls = () =>
  w.dataLayer
    .map((a) => Array.from(a as ArrayLike<unknown>))
    .filter((a) => a[0] === 'consent') as [string, string, Consent][];

describe('Google Consent Mode wiring in index.html', () => {
  beforeAll(() => {
    // Everything except the GTM loader, which would inject a network script.
    inline
      .filter((s) => !s.includes('googletagmanager'))
      .forEach((s) => (0, eval)(s));
  });

  it('sets the denied default before GTM loads', () => {
    const defaultAt = inline.findIndex((s) => s.includes("'default'"));
    const gtmAt = inline.findIndex((s) => s.includes('googletagmanager'));
    expect(defaultAt).toBeGreaterThanOrEqual(0);
    expect(defaultAt).toBeLessThan(gtmAt);
    expect(consentCalls()[0][1]).toBe('default');
    expect(consentCalls()[0][2]).toMatchObject({
      analytics_storage: 'denied',
      ad_storage: 'denied',
    });
  });

  it('blocks Custom HTML and Custom JS tags before GTM loads (lore-0593)', () => {
    const blockAt = inline.findIndex((s) => s.includes('gtm.blocklist'));
    const gtmAt = inline.findIndex((s) => s.includes('googletagmanager'));
    expect(blockAt).toBeGreaterThanOrEqual(0);
    expect(blockAt).toBeLessThan(gtmAt);
    expect(w.dataLayer).toContainEqual({ 'gtm.blocklist': ['customScripts'] });
  });

  it('forwards the banner categories as a consent update', () => {
    const [name, listener] = w._hsp[0];
    expect(name).toBe('addPrivacyConsentListener');

    listener({ categories: { analytics: true, advertisement: false } });
    expect(consentCalls().at(-1)).toEqual([
      'consent',
      'update',
      {
        analytics_storage: 'granted',
        ad_storage: 'denied',
        ad_user_data: 'denied',
        ad_personalization: 'denied',
      },
    ]);

    listener({ categories: { analytics: false, advertisement: false } });
    expect(consentCalls().at(-1)?.[2].analytics_storage).toBe('denied');
  });
});
