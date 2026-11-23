import { describe, expect, it } from 'vitest';

import {
  API_HUB_PAGES,
  apiHubRoutingFunctionCode,
} from '../api-hub-routing.js';
import type { HostMove } from '../legacy-host-redirect.js';

type Query = Record<
  string,
  { value: string; multiValue?: { value: string }[] }
>;

interface Redirect {
  statusCode: number;
  headers: { location: { value: string } };
}

interface Pass {
  uri: string;
  querystring?: Query;
}

type Handler = (event: unknown) => Redirect | Pass;

const handlerFor = (move?: HostMove) =>
  new Function(
    `${apiHubRoutingFunctionCode(move)}; return handler;`
  )() as Handler;

const handler = handlerFor();

const run = (h: Handler, uri: string, querystring: Query = {}, host?: string) =>
  h({
    request: {
      uri,
      querystring,
      headers: host ? { host: { value: host } } : {},
    },
  });

const pass = (uri: string, querystring: Query = {}, h: Handler = handler) => {
  const out = run(h, uri, querystring);
  expect(out).not.toHaveProperty('statusCode');
  return out as Pass;
};

const redirect = (
  uri: string,
  querystring: Query = {},
  h: Handler = handler,
  host?: string
) => {
  const out = run(h, uri, querystring, host);
  expect(out).toHaveProperty('statusCode');
  return out as Redirect;
};

describe('api-hub-routing', () => {
  it('knows the six pages of the hub', () => {
    expect([...API_HUB_PAGES].sort()).toEqual([
      'dashboard',
      'docs',
      'login',
      'prices-api',
      'privacy-policy',
      'quick-start',
    ]);
  });

  it.each([
    '/api/',
    '/api/login',
    '/api/login/',
    '/api/dashboard',
    '/api/dashboard/x402',
    '/api/quick-start/prices',
    '/api/docs/x402',
    '/api/docs/prices/anything',
    '/api/privacy-policy',
    '/api/prices-api/',
    '/api/prices-api/login',
  ])('%s is the hub page', (uri) => {
    expect(pass(uri).uri).toBe('/api/index.html');
  });

  it.each([
    '/api/docs/x402/openapi.json',
    '/api/prices-api/openapi.json',
    '/api/docs/prices/v1.2',
  ])('%s goes to the bucket as asked', (uri) => {
    expect(pass(uri).uri).toBe(uri);
  });

  it.each([
    ['/api', '/api/'],
    ['/api/prices-api', '/api/prices-api/'],
    ['/api/openapi.json', '/api/prices-api/openapi.json'],
  ])('%s answers 301 to %s', (uri, location) => {
    const out = redirect(uri);
    expect(out.statusCode).toBe(301);
    expect(out.headers.location.value).toBe(location);
  });

  it.each([
    ['/api', '/api/'],
    ['/api/prices-api', '/api/prices-api/'],
    ['/api/openapi.json', '/api/prices-api/openapi.json'],
  ])('%s keeps its query in the 301', (uri, location) => {
    const out = redirect(uri, { x: { value: '1' } });
    expect(out.headers.location.value).toBe(`${location}?x=1`);
  });

  it.each([
    '/api/x402-facilitator',
    '/api/x402-facilitator/',
    '/api/x402-facilitator/dashboard',
    '/api/foo',
    '/api/foo.js',
    '/api/loginx',
    '/api/hub',
    '/api/LOGIN',
    '/api//login',
  ])('%s answers 302 to /api/, without a query', (uri) => {
    const out = redirect(uri, { x: { value: '1' } });
    expect(out.statusCode).toBe(302);
    expect(out.headers.location.value).toBe('/api/');
  });

  it('leaves the query of a page address alone', () => {
    const out = pass('/api/login', { signin: { value: 'busy' } });
    expect(out.uri).toBe('/api/index.html');
    expect(out.querystring).toEqual({ signin: { value: 'busy' } });
  });

  describe('with the old host moved', () => {
    const move: HostMove = {
      from: 'sorobanscan.rumblefish.dev',
      to: 'sorobanscan.com',
    };
    const moved = handlerFor(move);
    const old = 'sorobanscan.rumblefish.dev';

    it('sends a page to the new host in one hop, query kept', () => {
      const out = redirect(
        '/api/login',
        { from: { value: 'prices' } },
        moved,
        old
      );
      expect(out.statusCode).toBe(301);
      expect(out.headers.location.value).toBe(
        'https://sorobanscan.com/api/login?from=prices'
      );
    });

    it('sends a path nobody serves to the new hub in one hop', () => {
      const out = redirect('/api/foo', {}, moved, old);
      expect(out.statusCode).toBe(302);
      expect(out.headers.location.value).toBe('https://sorobanscan.com/api/');
    });

    it('gives a bare /api its slash in the same hop', () => {
      expect(redirect('/api', {}, moved, old).headers.location.value).toBe(
        'https://sorobanscan.com/api/'
      );
      expect(
        redirect('/api/prices-api', {}, moved, old).headers.location.value
      ).toBe('https://sorobanscan.com/api/prices-api/');
    });

    it('moves the legacy openapi.json in one hop', () => {
      expect(
        redirect('/api/openapi.json', {}, moved, old).headers.location.value
      ).toBe('https://sorobanscan.com/api/prices-api/openapi.json');
    });

    it('answers as on a plain host on the new host', () => {
      const host = 'sorobanscan.com';
      expect(run(moved, '/api/login', {}, host)).toMatchObject({
        uri: '/api/index.html',
      });
      const out = run(moved, '/api/foo', {}, host) as Redirect;
      expect(out.statusCode).toBe(302);
      expect(out.headers.location.value).toBe('/api/');
    });
  });
});
