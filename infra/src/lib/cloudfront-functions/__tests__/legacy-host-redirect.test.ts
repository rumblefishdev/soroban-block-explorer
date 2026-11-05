import { describe, expect, it } from 'vitest';

import { legacyHostRedirectFunctionCode } from '../legacy-host-redirect.js';

type Response = {
  uri?: string;
  statusCode?: number;
  headers?: { location: { value: string } };
};

const handler = new Function(
  `${legacyHostRedirectFunctionCode({
    from: 'sorobanscan.rumblefish.dev',
    to: 'sorobanscan.com',
  })}; return handler;`
)() as (event: unknown) => Response;

const request = (
  host: string,
  uri: string,
  querystring: Record<
    string,
    { value: string; multiValue?: { value: string }[] }
  > = {}
) =>
  handler({
    request: { uri, querystring, headers: { host: { value: host } } },
  });

describe('legacy-host-redirect', () => {
  it.each([
    ['/', 'https://sorobanscan.com/'],
    ['/ledgers/123', 'https://sorobanscan.com/ledgers/123'],
    ['/api/', 'https://sorobanscan.com/api/'],
    ['/assets/index-abc.js', 'https://sorobanscan.com/assets/index-abc.js'],
  ])('the old host: %s → 301 %s', (uri, location) => {
    const response = request('sorobanscan.rumblefish.dev', uri);

    expect(response.statusCode).toBe(301);
    expect(response.headers?.location.value).toBe(location);
  });

  it('keeps the query string, repeated names included', () => {
    const response = request('Sorobanscan.Rumblefish.dev', '/tx', {
      utm_source: { value: 'x' },
      tag: { value: 'a', multiValue: [{ value: 'a' }, { value: 'b' }] },
    });

    expect(response.headers?.location.value).toBe(
      'https://sorobanscan.com/tx?utm_source=x&tag=a&tag=b'
    );
  });

  it('passes the new host through', () => {
    expect(request('sorobanscan.com', '/ledgers').uri).toBe('/ledgers');
  });
});
