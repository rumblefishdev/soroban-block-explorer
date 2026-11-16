import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import type { EnvironmentConfig } from '../lib/types.js';

// The behaviors under `/api/` on the SPA distribution (task 0670): the Prices
// portal, the x402 portal behind its own Basic Auth key, and the hub, all
// above `/api/*`.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const envs = {
  production: require('../../envs/production.json') as EnvironmentConfig,
  testnet: require('../../envs/testnet.json') as EnvironmentConfig,
};

type Resource = { Type: string; Properties: Record<string, unknown> };
type Behavior = {
  PathPattern: string;
  TargetOriginId: string;
  FunctionAssociations?: { FunctionARN: { 'Fn::GetAtt': [string, string] } }[];
};

function delivery(env: keyof typeof envs) {
  const out = mkdtempSync(join(tmpdir(), 'api-portal-behaviors-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({
    config: envs[env],
    cargoWorkspacePath: resolve(here, '../../..'),
  });
  const raw = readFileSync(
    join(out, `Explorer-${env}-Delivery.template.json`),
    'utf8'
  );
  const resources = (JSON.parse(raw) as { Resources: Record<string, Resource> })
    .Resources;
  const distribution = Object.values(resources).find(
    (r) => r.Type === 'AWS::CloudFront::Distribution'
  )?.Properties['DistributionConfig'] as { CacheBehaviors: Behavior[] };
  const behaviors = distribution.CacheBehaviors;

  /** The function a behavior runs: its name, source and KeyValueStores. */
  const functionOf = (pattern: string) => {
    const behavior = behaviors.find((b) => b.PathPattern === pattern);
    const id =
      behavior?.FunctionAssociations?.[0]?.FunctionARN['Fn::GetAtt'][0];
    const properties = (id ? resources[id]?.Properties : {}) as {
      Name?: string;
      FunctionCode?: unknown;
      FunctionConfig?: { KeyValueStoreAssociations?: unknown[] };
    };
    return {
      name: properties.Name,
      // With a KeyValueStore the source is an `Fn::Join` around the store's id.
      code: JSON.stringify(properties.FunctionCode),
      stores: properties.FunctionConfig?.KeyValueStoreAssociations ?? [],
      origin: behavior?.TargetOriginId,
    };
  };

  return { raw, patterns: behaviors.map((b) => b.PathPattern), functionOf };
}

describe('the /api/ behaviors in production', () => {
  const { raw, patterns, functionOf } = delivery('production');

  it('matches both portals and the hub before /api/*', () => {
    const rest = patterns.indexOf('/api/*');

    expect(rest).toBeGreaterThan(-1);
    for (const pattern of [
      '/api/prices-api',
      '/api/prices-api/*',
      '/api/x402-facilitator',
      '/api/x402-facilitator/*',
      '/api/',
    ]) {
      expect(patterns.indexOf(pattern), pattern).toBeGreaterThan(-1);
      expect(patterns.indexOf(pattern), pattern).toBeLessThan(rest);
    }
  });

  it('gates the x402 portal with a key of its own', () => {
    const bare = functionOf('/api/x402-facilitator');
    const nested = functionOf('/api/x402-facilitator/*');

    expect(bare.name).toBe('production-soroban-explorer-x402-portal-routing');
    expect(nested.name).toBe(bare.name);
    expect(nested.origin).toBe(functionOf('/api/prices-api/*').origin);
    expect(nested.stores).toHaveLength(1);
    expect(nested.code).toContain("kvs.get('auth-token-x402')");
    expect(nested.code).not.toContain("kvs.get('auth-token')");
    expect(nested.code).toContain('/api/x402-facilitator/index.html');
  });

  it('leaves the Prices portal public, on its own function', () => {
    const portal = functionOf('/api/prices-api/*');

    expect(portal.name).toBe('production-soroban-explorer-api-spa-routing');
    expect(functionOf('/api/prices-api').name).toBe(portal.name);
    expect(portal.stores).toHaveLength(0);
    expect(portal.code).not.toContain('kvs');
    expect(portal.code).toContain('/api/prices-api/index.html');
  });

  it('carries no credentials in the template', () => {
    expect(raw).not.toMatch(/Basic [A-Za-z0-9+/]{8,}={0,2}/);
  });
});

describe('the x402 portal on testnet', () => {
  it('is routed without the gate', () => {
    const nested = delivery('testnet').functionOf('/api/x402-facilitator/*');

    expect(nested.name).toBe('testnet-soroban-explorer-x402-portal-routing');
    expect(nested.stores).toHaveLength(0);
    expect(nested.code).not.toContain('kvs');
  });
});
