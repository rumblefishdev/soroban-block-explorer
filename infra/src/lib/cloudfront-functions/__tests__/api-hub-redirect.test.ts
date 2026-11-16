import { describe, expect, it } from 'vitest';

import {
  apiHubRedirectFunctionCode,
  PRICES_PORTAL_LEGACY_ROUTES,
} from '../api-hub-redirect.js';
import type { HostMove } from '../legacy-host-redirect.js';

type Query = Record<
  string,
  { value: string; multiValue?: { value: string }[] }
>;

type Handler = (event: unknown) => {
  statusCode: number;
  headers: { location: { value: string } };
};

const handlerFor = (portalMove?: HostMove, hubMove?: HostMove) =>
  new Function(
    `${apiHubRedirectFunctionCode(portalMove, hubMove)}; return handler;`
  )() as Handler;

const handler = handlerFor();

const redirect = (uri: string, querystring: Query = {}) =>
  handler({ request: { uri, querystring } });

// `/api`, `/api/`, `/api/prices-api…` and `/api/x402-facilitator…` have
// behaviors of their own and never reach this function.
describe('api-hub-redirect', () => {
  it('knows the routes the Prices portal had at /api/', () => {
    expect([...PRICES_PORTAL_LEGACY_ROUTES].sort()).toEqual([
      'dashboard',
      'docs',
      'login',
      'openapi.json',
      'privacy-policy',
      'quick-start',
    ]);
  });

  it.each([
    ...PRICES_PORTAL_LEGACY_ROUTES.map((route) => [
      `/api/${route}`,
      `/api/prices-api/${route}`,
    ]),
    ['/api/dashboard/', '/api/prices-api/dashboard/'],
    ['/api/docs/assets/price', '/api/prices-api/docs/assets/price'],
  ])('%s → 301 %s', (uri, location) => {
    const response = redirect(uri as string);

    expect(response.statusCode).toBe(301);
    expect(response.headers.location.value).toBe(location);
  });

  it('keeps the query string of a portal route, repeated names included', () => {
    const response = redirect('/api/dashboard', {
      signin: { value: 'failed' },
      tag: { value: 'a', multiValue: [{ value: 'a' }, { value: 'b' }] },
    });

    expect(response.headers.location.value).toBe(
      '/api/prices-api/dashboard?signin=failed&tag=a&tag=b'
    );
  });

  it.each([
    '/api/anything',
    '/api/anything/nested',
    '/api/sorobanscan/',
    '/api/assets/index-abc.js',
    '/api/index.html',
    // A name that only starts like a portal route.
    '/api/docs-old',
    '/api/dashboard.html',
    '/api/Dashboard',
    '/api//dashboard',
  ])('%s → 302 /api/', (uri) => {
    const response = redirect(uri, { x: { value: '1' } });

    expect(response.statusCode).toBe(302);
    expect(response.headers.location.value).toBe('/api/');
  });

  // Task 0664: an old link on the old host takes one hop, not two.
  describe('with the old host moved', () => {
    const move = { from: 'sorobanscan.rumblefish.dev', to: 'sorobanscan.com' };
    const moved = handlerFor(move, move);
    const on = (host: string, uri: string) =>
      moved({
        request: {
          uri,
          querystring: { x: { value: '1' } },
          headers: { host: { value: host } },
        },
      }).headers.location.value;

    it('sends the old host straight to the new one', () => {
      expect(on('sorobanscan.rumblefish.dev', '/api/dashboard')).toBe(
        'https://sorobanscan.com/api/prices-api/dashboard?x=1'
      );
      expect(on('sorobanscan.rumblefish.dev', '/api/anything')).toBe(
        'https://sorobanscan.com/api/'
      );
    });

    it('keeps the new host relative', () => {
      expect(on('sorobanscan.com', '/api/dashboard')).toBe(
        '/api/prices-api/dashboard?x=1'
      );
      expect(on('sorobanscan.com', '/api/anything')).toBe('/api/');
    });
  });

  // While the Prices portal stays on the old host, its routes go there with a
  // 302, and the hub stays where the main SPA is: on the new host.
  describe('with the portal kept on the old host', () => {
    const kept = handlerFor(
      {
        from: 'sorobanscan.com',
        to: 'sorobanscan.rumblefish.dev',
        temporary: true,
      },
      { from: 'sorobanscan.rumblefish.dev', to: 'sorobanscan.com' }
    );
    const on = (host: string, uri: string) =>
      kept({ request: { uri, headers: { host: { value: host } } } });

    it('sends a portal route on the new host back with a 302', () => {
      const response = on('sorobanscan.com', '/api/dashboard');

      expect(response.statusCode).toBe(302);
      expect(response.headers.location.value).toBe(
        'https://sorobanscan.rumblefish.dev/api/prices-api/dashboard'
      );
    });

    it('sends the rest to the hub on the new host with a 302', () => {
      const fromOld = on('sorobanscan.rumblefish.dev', '/api/anything');
      expect(fromOld.statusCode).toBe(302);
      expect(fromOld.headers.location.value).toBe(
        'https://sorobanscan.com/api/'
      );

      const fromNew = on('sorobanscan.com', '/api/anything');
      expect(fromNew.statusCode).toBe(302);
      expect(fromNew.headers.location.value).toBe('/api/');
    });
  });
});
