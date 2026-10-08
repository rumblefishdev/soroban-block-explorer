import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import type { EnvironmentConfig } from '../lib/types.js';

// Every Lambda that asks a Soroban RPC gets its network's pool from config
// (lore-0553): the code holds no default, so a Lambda without the env would
// refuse to start, and a mainnet default once sent the testnet worker to
// mainnet. Synthesised without bundling the Rust Lambdas.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;
const testnet = require('../../envs/testnet.json') as EnvironmentConfig;

type Resource = { Type: string; Properties: Record<string, unknown> };

// Environment variables of each Lambda in the Compute stack, by function name
// (CDK's own helper Lambdas carry none and are left out).
function lambdaEnv(
  config: EnvironmentConfig
): Record<string, Record<string, string>> {
  const out = mkdtempSync(join(tmpdir(), 'rpc-urls-synth-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({ config, cargoWorkspacePath: resolve(here, '../../..') });
  const template = JSON.parse(
    readFileSync(
      join(out, `Explorer-${config.envName}-Compute.template.json`),
      'utf8'
    )
  ) as { Resources: Record<string, Resource> };
  return Object.fromEntries(
    Object.values(template.Resources)
      .filter(
        (r) => r.Type === 'AWS::Lambda::Function' && r.Properties['Environment']
      )
      .map((f) => [
        f.Properties['FunctionName'] as string,
        (f.Properties['Environment'] as { Variables: Record<string, string> })
          .Variables,
      ])
  );
}

describe.each([
  ['production', production],
  ['testnet', testnet],
])('%s Soroban RPC pool', (envName, config) => {
  it('reaches the API and the enrichment worker from its own config', () => {
    const env = lambdaEnv(config);
    const pool = config.sorobanRpcUrls.join(',');
    expect(env[`${envName}-soroban-explorer-api`]?.['SOROBAN_RPC_URLS']).toBe(
      pool
    );
    expect(
      env[`${envName}-soroban-explorer-enrichment-worker`]?.['SOROBAN_RPC_URLS']
    ).toBe(pool);
  });
});

it('testnet lists no mainnet endpoint', () => {
  for (const url of testnet.sorobanRpcUrls) {
    expect(production.sorobanRpcUrls).not.toContain(url);
  }
});
