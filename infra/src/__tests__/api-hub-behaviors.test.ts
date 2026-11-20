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

function delivery(env: keyof typeof envs, hub?: Partial<typeof apiHub>) {
  const out = mkdtempSync(join(tmpdir(), 'api-hub-behaviors-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  const config = {
    ...envs[env],
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
  ] as { CacheBehaviors: Behavior[]; Origins: Behavior[] };
  const behaviors = dist.CacheBehaviors;
  const behavior = (pattern: string) =>
    behaviors.find((b) => b['PathPattern'] === pattern) as Behavior;
  const functionName = (pattern: string) => {
    const id = behavior(pattern)?.['FunctionAssociations']?.[0]?.FunctionARN[
      'Fn::GetAtt'
    ][0] as string | undefined;
    return id ? (resources[id]?.Properties['Name'] as string) : undefined;
  };
  return {
    raw,
    resources,
    distConfig: dist,
    find,
    dist,
    behavior,
    functionName,
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
