import { describe, expect, it } from 'vitest';

import { apiSpaRoutingFunctionCode } from '../api-spa-routing.js';

// Without a KVS the source has no `import`, so it evaluates as a plain script.
const handler = new Function(
  `${apiSpaRoutingFunctionCode(undefined)}; return handler;`
)() as (event: unknown) => Promise<{
  uri?: string;
  statusCode?: number;
  headers?: { location: { value: string } };
}>;

const route = (uri: string) => handler({ request: { uri, headers: {} } });

// Task 0664: the old host answers `301` to the new one, bare prefix included.
const moved = new Function(
  `${apiSpaRoutingFunctionCode(undefined, {
    from: 'sorobanscan.rumblefish.dev',
    to: 'sorobanscan.com',
  })}; return handler;`
)() as typeof handler;
const routeOn = (host: string, uri: string) =>
  moved({
    request: {
      uri,
      querystring: { signin: { value: 'ok' } },
      headers: { host: { value: host } },
    },
  });

describe('api-spa-routing', () => {
  it('redirects the bare prefix to its trailing slash', async () => {
    const response = await route('/api/prices-api');

    expect(response.statusCode).toBe(301);
    expect(response.headers?.location.value).toBe('/api/prices-api/');
  });

  it.each(['/api/prices-api/', '/api/prices-api/dashboard/keys'])(
    '%s → the portal index',
    async (uri) => {
      expect((await route(uri)).uri).toBe('/api/prices-api/index.html');
    }
  );

  it('passes static assets through', async () => {
    const uri = '/api/prices-api/assets/index-abc.js';

    expect((await route(uri)).uri).toBe(uri);
  });

  it.each([
    ['/api/prices-api', 'https://sorobanscan.com/api/prices-api/?signin=ok'],
    [
      '/api/prices-api/dashboard',
      'https://sorobanscan.com/api/prices-api/dashboard?signin=ok',
    ],
  ])('the old host: %s → 301 %s', async (uri, location) => {
    const response = await routeOn('sorobanscan.rumblefish.dev', uri);

    expect(response.statusCode).toBe(301);
    expect(response.headers?.location.value).toBe(location);
  });

  it('sends the new host back while the portal stays on the old one', async () => {
    const kept = new Function(
      `${apiSpaRoutingFunctionCode(undefined, {
        from: 'sorobanscan.com',
        to: 'sorobanscan.rumblefish.dev',
        temporary: true,
      })}; return handler;`
    )() as typeof handler;
    const on = (host: string, uri: string) =>
      kept({ request: { uri, headers: { host: { value: host } } } });

    const back = await on('sorobanscan.com', '/api/prices-api/dashboard');
    expect(back.statusCode).toBe(302);
    expect(back.headers?.location.value).toBe(
      'https://sorobanscan.rumblefish.dev/api/prices-api/dashboard'
    );

    const served = await on(
      'sorobanscan.rumblefish.dev',
      '/api/prices-api/keys'
    );
    expect(served.uri).toBe('/api/prices-api/index.html');
  });

  it('serves the new host as before', async () => {
    expect((await routeOn('sorobanscan.com', '/api/prices-api/docs')).uri).toBe(
      '/api/prices-api/index.html'
    );
  });
});
