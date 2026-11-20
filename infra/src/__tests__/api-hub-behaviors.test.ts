import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import type { EnvironmentConfig } from '../lib/types.js';

// The API hub's backend behavior, `/api/hub/*`, added by a setting (task 0680).
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const envs = {
  production: require('../../envs/production.json') as EnvironmentConfig,
  testnet: require('../../envs/testnet.json') as EnvironmentConfig,
};

const apiHub = {
  backendHost: 'abc123.lambda-url.eu-central-1.on.aws',
  originSecretName: 'sorobanscan-hub/production/origin-secret',
  originSecretRevision: '1',
  pages: false,
};

// The template is read as loose JSON.
/* eslint-disable @typescript-eslint/no-explicit-any */
type Resource = { Type: string; Properties: Record<string, any> };
type Behavior = Record<string, any>;

function delivery(
  env: keyof typeof envs,
  hub?: Partial<typeof apiHub>,
  more?: Partial<EnvironmentConfig>
) {
  const out = mkdtempSync(join(tmpdir(), 'api-hub-behaviors-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  const config = {
    ...envs[env],
    ...more,
    ...(hub && { apiHub: { ...apiHub, ...hub } }),
  };
  createApp({ config, cargoWorkspacePath: resolve(here, '../../..') });
  const raw = readFileSync(
    join(out, `Explorer-${env}-Delivery.template.json`),
    'utf8'
  );
  const resources = (JSON.parse(raw) as { Resources: Record<string, Resource> })
    .Resources;
  const find = (type: string) =>
    Object.values(resources).filter((r) => r.Type === type);
  const dist = find('AWS::CloudFront::Distribution')[0]?.Properties[
    'DistributionConfig'
  ] as {
    CacheBehaviors: Behavior[];
    Origins: Behavior[];
    DefaultCacheBehavior: Behavior;
  };
  const behaviors = dist.CacheBehaviors;
  const behavior = (pattern: string) =>
    behaviors.find((b) => b['PathPattern'] === pattern) as Behavior;
  const functionId = (pattern: string) =>
    behavior(pattern)?.['FunctionAssociations']?.[0]?.FunctionARN[
      'Fn::GetAtt'
    ][0] as string | undefined;
  const functionName = (pattern: string) => {
    const id = functionId(pattern);
    return id ? (resources[id]?.Properties['Name'] as string) : undefined;
  };
  const functionCode = (pattern: string) =>
    resources[functionId(pattern) ?? '']?.Properties['FunctionCode'] as string;
  /** The bucket a behavior's origin is (its logical id), or the origin's host. */
  const originOf = (pattern: string) => {
    const origin = dist.Origins.find(
      (o) => o['Id'] === behavior(pattern)['TargetOriginId']
    ) as Behavior;
    return (origin['DomainName']['Fn::GetAtt']?.[0] ??
      origin['DomainName']) as string;
  };
  /** A behavior's response-headers policy; the default behavior's without a pattern. */
  const headersOf = (pattern?: string) => {
    const b = pattern ? behavior(pattern) : dist.DefaultCacheBehavior;
    return resources[b['ResponseHeadersPolicyId'].Ref]?.Properties[
      'ResponseHeadersPolicyConfig'
    ];
  };
  return {
    raw,
    resources,
    distConfig: dist,
    find,
    dist,
    behavior,
    functionId,
    functionName,
    functionCode,
    originOf,
    headersOf,
    patterns: behaviors.map((b) => b['PathPattern'] as string),
  };
}

const TODAY = [
  '/assets/*',
  '/static/*',
  '/api/prices-api',
  '/api/prices-api/*',
  '/api/x402-facilitator',
  '/api/x402-facilitator/*',
  '/api/',
  '/api/*',
  '/prices-api',
  '/prices-api/*',
  '/pricing-api',
  '/pricing-api/*',
];

describe('without apiHub', () => {
  for (const env of ['production', 'testnet'] as const) {
    it(`${env} keeps today's behaviors and their functions`, () => {
      const d = delivery(env);

      expect(d.patterns).toEqual(TODAY);
      for (const [pattern, name] of Object.entries({
        '/api/prices-api': 'api-spa-routing',
        '/api/prices-api/*': 'api-spa-routing',
        '/api/x402-facilitator': 'x402-portal-routing',
        '/api/x402-facilitator/*': 'x402-portal-routing',
        '/api/*': 'api-hub-redirect',
      })) {
        expect(d.functionName(pattern), pattern).toBe(
          `${env}-soroban-explorer-${name}`
        );
      }
      expect(d.raw).not.toContain('X-Hub-Origin');
      expect(d.raw).not.toContain('api-hub-backend');
    });
  }
});

describe('with the backend alone', () => {
  const d = delivery('production', { pages: false });
  const b = d.behavior('/api/hub/*');

  it('puts /api/hub/* before /api/* and changes nothing else', () => {
    const expected = [...TODAY];
    expected.splice(expected.indexOf('/api/*'), 0, '/api/hub/*');
    expect(d.patterns).toEqual(expected);
  });

  it('changes nothing else in the distribution or the stack', () => {
    const before = delivery('production');
    const hubOrigin = b['TargetOriginId'];
    const rest = {
      ...d.distConfig,
      CacheBehaviors: d.distConfig.CacheBehaviors.filter(
        (x) => x['PathPattern'] !== '/api/hub/*'
      ),
      Origins: d.distConfig.Origins.filter((o) => o['Id'] !== hubOrigin),
    };
    expect(rest).toEqual(before.distConfig);

    const added = Object.keys(d.resources).filter(
      (k) => !(k in before.resources)
    );
    expect(added.map((k) => d.resources[k]?.Type).sort()).toEqual([
      'AWS::CloudFront::Function',
      'AWS::CloudFront::OriginRequestPolicy',
    ]);
    for (const k of Object.keys(before.resources)) {
      if (before.resources[k]?.Type === 'AWS::CloudFront::Distribution')
        continue;
      expect(d.resources[k], k).toEqual(before.resources[k]);
    }
  });

  it('points at the backend over HTTPS with the secret and revision', () => {
    const origin = d.dist.Origins.find(
      (o) => o['Id'] === b['TargetOriginId']
    ) as Behavior;
    const headers = origin['OriginCustomHeaders'] as {
      HeaderName: string;
      HeaderValue: unknown;
    }[];
    const secret = headers.find((h) => h.HeaderName === 'X-Hub-Origin-Secret');
    const revision = headers.find(
      (h) => h.HeaderName === 'X-Hub-Origin-Revision'
    );

    expect(origin['DomainName']).toBe(apiHub.backendHost);
    expect(origin['CustomOriginConfig'].OriginProtocolPolicy).toBe(
      'https-only'
    );
    expect(origin['CustomOriginConfig'].OriginReadTimeout).toBe(30);
    expect(JSON.stringify(secret?.HeaderValue)).toContain(
      '{{resolve:secretsmanager:'
    );
    expect(JSON.stringify(secret?.HeaderValue)).toContain(
      apiHub.originSecretName
    );
    expect(JSON.stringify(secret?.HeaderValue)).toContain(
      ':SecretString:current'
    );
    expect(JSON.stringify(secret?.HeaderValue)).not.toContain('arn:');
    expect(revision?.HeaderValue).toBe('1');
  });

  it('does not cache, allows every method, HTTPS only', () => {
    expect(b['CachePolicyId']).toBe('4135ea2d-6df8-44a3-9df3-4b5a84be39ad');
    expect([...b['AllowedMethods']].sort()).toEqual(
      ['DELETE', 'GET', 'HEAD', 'OPTIONS', 'PATCH', 'POST', 'PUT'].sort()
    );
    expect(b['ViewerProtocolPolicy']).toBe('https-only');
  });

  it('forwards every cookie, query string and exactly four headers', () => {
    const policy = d
      .find('AWS::CloudFront::OriginRequestPolicy')
      .find((p) => p.Properties['OriginRequestPolicyConfig']);
    const config = d
      .find('AWS::CloudFront::OriginRequestPolicy')
      .map((p) => p.Properties['OriginRequestPolicyConfig'])
      .find((c) => c.Name?.includes('hub'));

    expect(policy).toBeDefined();
    expect(config.CookiesConfig.CookieBehavior).toBe('all');
    expect(config.QueryStringsConfig.QueryStringBehavior).toBe('all');
    expect([...config.HeadersConfig.Headers].sort()).toEqual(
      ['Accept', 'Content-Type', 'Origin', 'X-Hub-Viewer-Ip'].sort()
    );
    expect(b['OriginRequestPolicyId']).toBeDefined();
  });

  it("strips the viewer's own trusted headers before CloudFront adds its own", () => {
    expect(d.functionName('/api/hub/*')).toBe(
      'production-soroban-explorer-api-hub-backend-headers'
    );
  });

  it('gives a different template for a different revision', () => {
    const other = delivery('production', {
      pages: false,
      originSecretRevision: '2',
    });
    expect(other.raw).not.toBe(d.raw);
  });
});

// The second stage, the start: the hub's bundle serves `/api/` (task 0680).
const underApi = (pattern: string) =>
  pattern === '/api' || pattern.startsWith('/api/');
// The bucket's behaviors come before the backend's, so the bucket's origin
// (and its origin access control) keeps the number it has with the pages off.
const HUB = [
  '/api/assets/*',
  '/api/prices-api',
  '/api/prices-api/*',
  '/api/hub/*',
  '/api',
  '/api/*',
];
const HUB_PAGES = [
  'login',
  'dashboard',
  'quick-start',
  'docs',
  'privacy-policy',
  'prices-api',
];
const CACHING_OPTIMIZED = '658327ea-f89d-4fab-a63d-7e88639e58f6';

describe('with both stages on', () => {
  const d = delivery('production', { pages: true });
  const off = delivery('production', { pages: false });

  it("puts the hub's behaviors under /api, /api/* last, and keeps the rest", () => {
    expect(d.patterns.filter(underApi)).toEqual(HUB);
    for (const gone of [
      '/api/x402-facilitator',
      '/api/x402-facilitator/*',
      '/api/',
    ]) {
      expect(d.patterns, gone).not.toContain(gone);
    }
    expect(d.patterns).toEqual([
      ...TODAY.filter((p) => !underApi(p)).slice(0, 2),
      ...HUB,
      ...TODAY.filter((p) => !underApi(p)).slice(2),
    ]);
  });

  it("runs the hub's function on /api, /api/* and the Prices pages, from the bucket", () => {
    for (const pattern of [
      '/api',
      '/api/*',
      '/api/prices-api',
      '/api/prices-api/*',
    ]) {
      expect(d.functionName(pattern), pattern).toBe(
        'production-soroban-explorer-api-hub-routing'
      );
      expect(d.functionId(pattern), pattern).toBe(d.functionId('/api/*'));
      expect(d.originOf(pattern), pattern).toMatch(/^ApiSpaBucket/);
      expect(JSON.stringify(d.behavior(pattern)['CachePolicyId'])).toContain(
        'ShortTtlCachePolicy'
      );
    }
    expect(d.functionCode('/api/*')).toContain(JSON.stringify(HUB_PAGES));
    // The old host's `301` comes first, as on every behavior.
    expect(d.functionCode('/api/*')).toContain(
      `'${envs.production.legacyDomainName}'`
    );
  });

  it('serves /api/assets/* from the bucket with the long cache of /assets/*', () => {
    expect(d.originOf('/api/assets/*')).toMatch(/^ApiSpaBucket/);
    expect(d.behavior('/api/assets/*')['CachePolicyId']).toBe(
      CACHING_OPTIMIZED
    );
    // No function of its own: the main behaviors' one, the old host's `301`,
    // as `/assets/*` runs it.
    expect(d.functionName('/api/assets/*')).toBe(
      'production-soroban-explorer-legacy-host-redirect'
    );
    expect(d.functionId('/api/assets/*')).toBe(d.functionId('/assets/*'));
  });

  it('marks the Prices pages noindex, with every security header of the rest', () => {
    const shared = d.headersOf();
    const prices = d.headersOf('/api/prices-api/*');

    expect(d.headersOf('/api/prices-api')).toEqual(prices);
    expect(prices.CustomHeadersConfig.Items).toEqual([
      { Header: 'X-Robots-Tag', Value: 'noindex', Override: true },
    ]);
    expect(prices.SecurityHeadersConfig).toEqual(shared.SecurityHeadersConfig);
    expect(JSON.stringify(shared)).not.toContain('X-Robots-Tag');
    for (const pattern of d.patterns.filter(
      (p) => !p.startsWith('/api/prices-api')
    )) {
      expect(JSON.stringify(d.headersOf(pattern)), pattern).not.toContain(
        'X-Robots-Tag'
      );
    }
  });

  it("drops the portals' functions and keeps the old homes' redirect", () => {
    const names = d
      .find('AWS::CloudFront::Function')
      .map((f) => f.Properties['Name'] as string);

    for (const gone of [
      'x402-portal-routing',
      'api-spa-routing',
      'api-hub-redirect',
    ]) {
      expect(names).not.toContain(`production-soroban-explorer-${gone}`);
    }
    expect(names).toContain('production-soroban-explorer-api-path-redirect');
    expect(d.functionName('/api/hub/*')).toBe(
      'production-soroban-explorer-api-hub-backend-headers'
    );
  });

  it('changes nothing outside /api', () => {
    const { CacheBehaviors: on, ...onRest } = d.distConfig;
    const { CacheBehaviors: was, ...offRest } = off.distConfig;
    expect(onRest).toEqual(offRest);
    expect(on.filter((b) => !underApi(b['PathPattern']))).toEqual(
      was.filter((b) => !underApi(b['PathPattern']))
    );
    expect(d.behavior('/api/hub/*')).toEqual(off.behavior('/api/hub/*'));

    const types = (keys: string[], from: Record<string, Resource>) =>
      keys.map((k) => from[k]?.Type).sort();
    const added = Object.keys(d.resources).filter((k) => !(k in off.resources));
    const removed = Object.keys(off.resources).filter(
      (k) => !(k in d.resources)
    );
    expect(types(added, d.resources)).toEqual([
      'AWS::CloudFront::Function',
      'AWS::CloudFront::ResponseHeadersPolicy',
    ]);
    expect(types(removed, off.resources)).toEqual([
      'AWS::CloudFront::Function',
      'AWS::CloudFront::Function',
      'AWS::CloudFront::Function',
    ]);
    for (const k of Object.keys(off.resources)) {
      const type = off.resources[k]?.Type;
      if (!(k in d.resources)) continue;
      // The CDK's own usage record lists the constructs, which differ.
      if (type === 'AWS::CDK::Metadata') continue;
      if (type === 'AWS::CloudFront::Distribution') continue;
      expect(d.resources[k], k).toEqual(off.resources[k]);
    }
  });

  it('names the function after the environment', () => {
    const testnet = delivery('testnet', { pages: true });
    expect(testnet.functionName('/api/*')).toBe(
      'testnet-soroban-explorer-api-hub-routing'
    );
    expect(testnet.functionCode('/api/*')).toContain(
      `'${envs.testnet.legacyDomainName}'`
    );
  });
});

describe('both stages on, then the pages off', () => {
  it('gives the backend-only template back', () => {
    const before = delivery('production', { pages: false });
    delivery('production', { pages: true });
    const after = delivery('production', { pages: false });

    expect(after.raw).toBe(before.raw);
    expect(after.patterns.filter(underApi)).toEqual([
      ...TODAY.filter(underApi).slice(0, -1),
      '/api/hub/*',
      '/api/*',
    ]);
    expect(after.functionName('/api/*')).toBe(
      'production-soroban-explorer-api-hub-redirect'
    );
  });
});

describe('the pages with the Prices portal on the old host', () => {
  it('stops the synth, naming both settings', () => {
    expect(() =>
      delivery(
        'production',
        { pages: true },
        { pricesPortalOnLegacyHost: true }
      )
    ).toThrow(/apiHub\.pages[\s\S]*pricesPortalOnLegacyHost/);
  });
});
