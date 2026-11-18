import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import type { EnvironmentConfig } from '../lib/types.js';

// The raw execute-api address skips Cloudflare and spends the stage throttle
// real users share, so every environment closes it (task 0285) — also with
// API mTLS off, which is how both env files ship today.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;
const testnet = require('../../envs/testnet.json') as EnvironmentConfig;

function restApis(config: EnvironmentConfig): Record<string, unknown>[] {
  const out = mkdtempSync(join(tmpdir(), 'execute-api-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({ config, cargoWorkspacePath: resolve(here, '../../..') });
  const resources = JSON.parse(
    readFileSync(
      join(out, `Explorer-${config.envName}-ApiGateway.template.json`),
      'utf8'
    )
  ).Resources as Record<
    string,
    { Type: string; Properties: Record<string, unknown> }
  >;
  return Object.values(resources)
    .filter((r) => r.Type === 'AWS::ApiGateway::RestApi')
    .map((r) => r.Properties);
}

describe('raw execute-api endpoint', () => {
  it.each([production, testnet])(
    'is disabled in $envName, with API mTLS off',
    (config) => {
      expect(config.enableApiMtls).toBe(false);
      const apis = restApis(config);
      expect(apis).toHaveLength(1);
      expect(apis[0]?.['DisableExecuteApiEndpoint']).toBe(true);
    }
  );
});
