import { describe, expect, it } from 'vitest';

import { apiHubBackendHeadersFunctionCode } from '../api-hub-backend-headers.js';

type Headers = Record<string, { value: string }>;
type Handler = (event: unknown) => { headers: Headers; uri: string };

const handler = new Function(
  `${apiHubBackendHeadersFunctionCode()}; return handler;`
)() as Handler;

describe('apiHubBackendHeadersFunctionCode', () => {
  it("drops the viewer's own copies of the two headers CloudFront sets", () => {
    const out = handler({
      request: {
        uri: '/api/hub/v1/account',
        headers: {
          'x-hub-origin-secret': { value: 'guess' },
          'x-hub-origin-revision': { value: '1' },
          'cloudfront-viewer-address': { value: '203.0.113.9:1234' },
          cookie: { value: 'a=b' },
          accept: { value: 'application/json' },
        },
      },
    });

    expect(Object.keys(out.headers).sort()).toEqual(['accept', 'cookie']);
    expect(out.uri).toBe('/api/hub/v1/account');
  });

  it('passes a request with none of them as it is', () => {
    const out = handler({
      request: { uri: '/api/hub/x', headers: { accept: { value: '*/*' } } },
    });

    expect(out.headers).toEqual({ accept: { value: '*/*' } });
  });
});
