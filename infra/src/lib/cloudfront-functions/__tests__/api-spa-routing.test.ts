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
});
