import { describe, expect, it } from 'vitest';

import { apiHubBackendHeadersFunctionCode } from '../api-hub-backend-headers.js';

type Headers = Record<string, { value: string }>;
type Handler = (event: unknown) => { headers: Headers; uri: string };

const handler = new Function(
  `${apiHubBackendHeadersFunctionCode()}; return handler;`
)() as Handler;

describe('apiHubBackendHeadersFunctionCode', () => {
  it("drops the viewer's secret and revision, and replaces x-hub-viewer-ip", () => {
    const out = handler({
      viewer: { ip: '198.51.100.7' },
      request: {
        uri: '/api/hub/v1/account',
        headers: {
          'x-hub-origin-secret': { value: 'guess' },
          'x-hub-origin-revision': { value: '1' },
          'cloudfront-viewer-address': { value: '203.0.113.9:1234' },
          'x-hub-viewer-ip': { value: '1.1.1.1' },
          cookie: { value: 'a=b' },
          accept: { value: 'application/json' },
        },
      },
    });

    expect(Object.keys(out.headers).sort()).toEqual([
      'accept',
      'cloudfront-viewer-address',
      'cookie',
      'x-hub-viewer-ip',
    ]);
    expect(out.headers['x-hub-viewer-ip']?.value).toBe('198.51.100.7');
    expect(out.headers['cloudfront-viewer-address']?.value).toBe(
      '203.0.113.9:1234'
    );
    expect(out.uri).toBe('/api/hub/v1/account');
  });

  it('passes a request with none of them as it is', () => {
    const out = handler({
      viewer: { ip: '198.51.100.7' },
      request: { uri: '/api/hub/x', headers: { accept: { value: '*/*' } } },
    });

    expect(out.headers).toEqual({
      accept: { value: '*/*' },
      'x-hub-viewer-ip': { value: '198.51.100.7' },
    });
  });

  it('writes an IPv6 address as it is', () => {
    const out = handler({
      viewer: { ip: '2001:db8::1' },
      request: { uri: '/api/hub/x', headers: {} },
    });

    expect(out.headers['x-hub-viewer-ip']?.value).toBe('2001:db8::1');
  });
});
