import { describe, expect, it } from 'vitest';

import { API_PATH_REDIRECT_FUNCTION_CODE } from '../api-path-redirect.js';

type Query = Record<
  string,
  { value: string; multiValue?: { value: string }[] }
>;

const handler = new Function(
  `${API_PATH_REDIRECT_FUNCTION_CODE}; return handler;`
)() as (event: unknown) => {
  statusCode: number;
  headers: { location: { value: string } };
};

const redirect = (uri: string, querystring: Query = {}) =>
  handler({ request: { uri, querystring } });

describe('api-path-redirect', () => {
  it.each([
    ['/api', '/prices-api/'],
    ['/api/', '/prices-api/'],
    ['/api/dashboard', '/prices-api/dashboard'],
    ['/api/assets/index-abc.js', '/prices-api/assets/index-abc.js'],
    // The name the portal had for a few hours on 2026-10-02.
    ['/pricing-api', '/prices-api/'],
    ['/pricing-api/', '/prices-api/'],
    ['/pricing-api/docs', '/prices-api/docs'],
  ])('%s → 301 %s', (uri, location) => {
    const response = redirect(uri);

    expect(response.statusCode).toBe(301);
    expect(response.headers.location.value).toBe(location);
  });

  // The Prices backend sends the OAuth popup back to `/api/?signin=…` until
  // its own deploy moves it — the outcome is in the query string.
  it('keeps the query string, repeated names included', () => {
    const response = redirect('/api/', {
      issue: { value: 'too_young' },
      wait_secs: { value: '3600' },
      tag: {
        value: 'a',
        multiValue: [{ value: 'a' }, { value: 'b' }],
      },
    });

    expect(response.headers.location.value).toBe(
      '/prices-api/?issue=too_young&wait_secs=3600&tag=a&tag=b'
    );
  });
});
