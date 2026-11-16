import { describe, expect, it } from 'vitest';

import {
  type ApiSpaRouting,
  apiSpaRoutingFunctionCode,
} from '../api-spa-routing.js';

type Handler = (event: unknown) => Promise<{
  uri?: string;
  statusCode?: number;
  headers?: Record<string, { value: string }>;
  body?: string;
}>;

const PRICES = '/api/prices-api';
const X402 = '/api/x402-facilitator';

// Without a KVS the source has no `import`, so it evaluates as a plain script.
const handlerFor = (routing: ApiSpaRouting) =>
  new Function(
    `${apiSpaRoutingFunctionCode(routing)}; return handler;`
  )() as Handler;

const handler = handlerFor({ prefix: PRICES });

const route = (uri: string) => handler({ request: { uri, headers: {} } });

// Task 0664: the old host answers `301` to the new one, bare prefix included.
const moved = handlerFor({
  prefix: PRICES,
  move: { from: 'sorobanscan.rumblefish.dev', to: 'sorobanscan.com' },
});
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
    expect(response.headers?.['location']?.value).toBe('/api/prices-api/');
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
    expect(response.headers?.['location']?.value).toBe(location);
  });

  it('sends the new host back while the portal stays on the old one', async () => {
    const kept = handlerFor({
      prefix: PRICES,
      move: {
        from: 'sorobanscan.com',
        to: 'sorobanscan.rumblefish.dev',
        temporary: true,
      },
    });
    const on = (host: string, uri: string) =>
      kept({ request: { uri, headers: { host: { value: host } } } });

    const back = await on('sorobanscan.com', '/api/prices-api/dashboard');
    expect(back.statusCode).toBe(302);
    expect(back.headers?.['location']?.value).toBe(
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

// Task 0670: the same source serves the x402 portal under its own prefix.
describe('api-spa-routing on /api/x402-facilitator', () => {
  const x402 = handlerFor({ prefix: X402 });
  const at = (uri: string) => x402({ request: { uri, headers: {} } });

  it('redirects the bare prefix to its trailing slash', async () => {
    const response = await at(X402);

    expect(response.statusCode).toBe(301);
    expect(response.headers?.['location']?.value).toBe(`${X402}/`);
  });

  it.each([`${X402}/`, `${X402}/dashboard`, `${X402}/a/b`])(
    '%s → the portal index',
    async (uri) => {
      expect((await at(uri)).uri).toBe(`${X402}/index.html`);
    }
  );

  it.each([`${X402}/index.html`, `${X402}/assets/index-abc.js`])(
    'passes %s through',
    async (uri) => {
      expect((await at(uri)).uri).toBe(uri);
    }
  );

  it('sends the old host to the new one', async () => {
    const onOldHost = handlerFor({
      prefix: X402,
      move: { from: 'sorobanscan.rumblefish.dev', to: 'sorobanscan.com' },
    });
    const response = await onOldHost({
      request: {
        uri: X402,
        headers: { host: { value: 'sorobanscan.rumblefish.dev' } },
      },
    });

    expect(response.statusCode).toBe(301);
    expect(response.headers?.['location']?.value).toBe(
      `https://sorobanscan.com${X402}/`
    );
  });
});

// The gated source starts with `import cf from 'cloudfront'`, which only the
// CloudFront runtime resolves: the test hands the module in as an argument.
describe('api-spa-routing behind Basic Auth', () => {
  // base64('tester:not-a-real-password'), made up for this test.
  const TOKEN = 'dGVzdGVyOm5vdC1hLXJlYWwtcGFzc3dvcmQ=';

  const gated = (store: Record<string, string>, key = 'auth-token-x402') => {
    const read: string[] = [];
    const cf = {
      kvs: (id: string) => ({
        get: (name: string) => {
          read.push(`${id}/${name}`);
          return name in store
            ? Promise.resolve(store[name])
            : Promise.reject(new Error('KeyNotFound'));
        },
      }),
    };
    const code = apiSpaRoutingFunctionCode({
      prefix: X402,
      auth: { kvsId: 'kvs-id', key, realm: 'x402 facilitator' },
    });
    expect(code).toContain("import cf from 'cloudfront';");
    const handle = new Function(
      'cf',
      `${code.replace("import cf from 'cloudfront';", '')}; return handler;`
    )(cf) as Handler;
    const at = (uri: string, authorization?: string) =>
      handle({
        request: {
          uri,
          headers: authorization
            ? { authorization: { value: authorization } }
            : {},
        },
      });
    return { at, read };
  };

  it('answers 401 without credentials', async () => {
    const response = await gated({ 'auth-token-x402': TOKEN }).at(`${X402}/`);

    expect(response.statusCode).toBe(401);
    expect(response.headers?.['www-authenticate']?.value).toBe(
      'Basic realm="x402 facilitator"'
    );
    expect(response.uri).toBeUndefined();
  });

  it.each([
    'Basic d3Jvbmc6d3Jvbmc=',
    `Bearer ${TOKEN}`,
    `Basic ${TOKEN}x`,
    'Basic ',
  ])('answers 401 to %s', async (authorization) => {
    const response = await gated({ 'auth-token-x402': TOKEN }).at(
      `${X402}/`,
      authorization
    );

    expect(response.statusCode).toBe(401);
  });

  it('answers 503 while the key is not in the store', async () => {
    // The other gates' key is there; this gate must not fall back to it.
    const { at, read } = gated({ 'auth-token': TOKEN });
    const response = await at(`${X402}/`, `Basic ${TOKEN}`);

    expect(response.statusCode).toBe(503);
    expect(response.headers?.['cache-control']?.value).toBe('no-store');
    expect(read).toEqual(['kvs-id/auth-token-x402']);
  });

  it('serves the portal with the right credentials', async () => {
    const { at } = gated({ 'auth-token-x402': TOKEN });

    expect((await at(`${X402}/`, `Basic ${TOKEN}`)).uri).toBe(
      `${X402}/index.html`
    );
    expect((await at(`${X402}/assets/a.js`, `Basic ${TOKEN}`)).uri).toBe(
      `${X402}/assets/a.js`
    );
  });

  it('gates static files too', async () => {
    const response = await gated({ 'auth-token-x402': TOKEN }).at(
      `${X402}/index.html`
    );

    expect(response.statusCode).toBe(401);
  });

  it('reads the key it is given', async () => {
    const { at, read } = gated({ 'auth-token': TOKEN }, 'auth-token');

    expect((await at(`${X402}/`, `Basic ${TOKEN}`)).uri).toBe(
      `${X402}/index.html`
    );
    expect(read).toEqual(['kvs-id/auth-token']);
  });
});
