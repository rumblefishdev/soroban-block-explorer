import { describe, expect, it } from 'vitest';

import type { HostMove } from '../legacy-host-redirect.js';
import { apiHubBackendHeadersFunctionCode } from '../api-hub-backend-headers.js';

type Headers = Record<string, { value: string }>;
type Handler = (event: unknown) => {
  headers: Headers;
  uri: string;
  statusCode?: number;
};

const handlerFor = (move?: HostMove) =>
  new Function(
    `${apiHubBackendHeadersFunctionCode(move)}; return handler;`
  )() as Handler;

const handler = handlerFor();

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

  describe('with the old host moved', () => {
    const move: HostMove = { from: 'old.example', to: 'sorobanscan.com' };
    const moved = handlerFor(move);

    it("answers the old host's request with a 301 in one hop, query kept", () => {
      const out = moved({
        viewer: { ip: '198.51.100.7' },
        request: {
          uri: '/api/hub/me',
          querystring: { x: { value: '1' } },
          headers: { host: { value: 'old.example' } },
        },
      }) as unknown as {
        statusCode: number;
        headers: { location: { value: string } };
      };

      expect(out.statusCode).toBe(301);
      expect(out.headers.location.value).toBe(
        'https://sorobanscan.com/api/hub/me?x=1'
      );
    });

    it("passes the new host's request with the address written", () => {
      const out = moved({
        viewer: { ip: '198.51.100.7' },
        request: {
          uri: '/api/hub/me',
          headers: { host: { value: 'sorobanscan.com' } },
        },
      });

      expect(out.statusCode).toBeUndefined();
      expect(out.headers['x-hub-viewer-ip']?.value).toBe('198.51.100.7');
    });
  });
});
