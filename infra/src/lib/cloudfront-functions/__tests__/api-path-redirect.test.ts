import { describe, expect, it } from 'vitest';

import { apiPathRedirectFunctionCode } from '../api-path-redirect.js';

type Query = Record<
  string,
  { value: string; multiValue?: { value: string }[] }
>;

type Handler = (event: unknown) => {
  statusCode: number;
  headers: { location: { value: string } };
};

const handlerFor = (code: string) =>
  new Function(`${code}; return handler;`)() as Handler;

const handler = handlerFor(apiPathRedirectFunctionCode(undefined));

const redirect = (uri: string, querystring: Query = {}) =>
  handler({ request: { uri, querystring } });

describe('api-path-redirect', () => {
  it.each([
    // The portal's home from task 0608 until task 0660.
    ['/prices-api', '/api/prices-api/'],
    ['/prices-api/', '/api/prices-api/'],
    ['/prices-api/dashboard', '/api/prices-api/dashboard'],
    ['/prices-api/assets/index-abc.js', '/api/prices-api/assets/index-abc.js'],
    // The name the portal had for a few hours on 2026-10-02.
    ['/pricing-api', '/api/prices-api/'],
    ['/pricing-api/', '/api/prices-api/'],
    ['/pricing-api/docs', '/api/prices-api/docs'],
    // The portal's home until task 0608. `/api` and `/api/` are the hub and
    // never reach this function.
    ['/api/dashboard', '/api/prices-api/dashboard'],
    ['/api/assets/index-abc.js', '/api/prices-api/assets/index-abc.js'],
  ])('%s → 301 %s', (uri, location) => {
    const response = redirect(uri);

    expect(response.statusCode).toBe(301);
    expect(response.headers.location.value).toBe(location);
  });

  // The Prices backend sends the OAuth popup back to `/prices-api/?signin=…`
  // until its own deploy moves it — the outcome is in the query string.
  it('keeps the query string, repeated names included', () => {
    const response = redirect('/prices-api/', {
      issue: { value: 'too_young' },
      wait_secs: { value: '3600' },
      tag: {
        value: 'a',
        multiValue: [{ value: 'a' }, { value: 'b' }],
      },
    });

    expect(response.headers.location.value).toBe(
      '/api/prices-api/?issue=too_young&wait_secs=3600&tag=a&tag=b'
    );
  });

  // Task 0664: an old link on the old host takes one hop, not two.
  describe('with the old host moved', () => {
    const moved = handlerFor(
      apiPathRedirectFunctionCode({
        from: 'sorobanscan.rumblefish.dev',
        to: 'sorobanscan.com',
      })
    );
    const on = (host: string, uri: string, querystring: Query = {}) =>
      moved({
        request: { uri, querystring, headers: { host: { value: host } } },
      }).headers.location.value;

    it('sends the old host straight to the new one', () => {
      expect(
        on('sorobanscan.rumblefish.dev', '/prices-api/dashboard', {
          x: { value: '1' },
        })
      ).toBe('https://sorobanscan.com/api/prices-api/dashboard?x=1');
    });

    it('keeps the new host relative', () => {
      expect(on('sorobanscan.com', '/prices-api/dashboard')).toBe(
        '/api/prices-api/dashboard'
      );
    });
  });

  // While the portal stays on the old host: the new host sends old links
  // there, with a 302, and the old host stays relative.
  describe('with the portal kept on the old host', () => {
    const kept = handlerFor(
      apiPathRedirectFunctionCode({
        from: 'sorobanscan.com',
        to: 'sorobanscan.rumblefish.dev',
        temporary: true,
      })
    );
    const on = (host: string, uri: string) =>
      kept({
        request: {
          uri,
          querystring: { x: { value: '1' } },
          headers: { host: { value: host } },
        },
      });

    it('sends the new host back with a 302', () => {
      const response = on('sorobanscan.com', '/prices-api/dashboard');

      expect(response.statusCode).toBe(302);
      expect(response.headers.location.value).toBe(
        'https://sorobanscan.rumblefish.dev/api/prices-api/dashboard?x=1'
      );
    });

    it('keeps the old host relative, with a 301', () => {
      const response = on('sorobanscan.rumblefish.dev', '/pricing-api/docs');

      expect(response.statusCode).toBe(301);
      expect(response.headers.location.value).toBe('/api/prices-api/docs?x=1');
    });
  });
});
