import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import type { EnvironmentConfig } from '../lib/types.js';

// The API is one `{proxy+}` method that requires no API key, and API Gateway
// applies a usage plan only to keys on key-required methods, so a partner
// plan, key and quota here count and limit nothing (task 0679). The paid
// tier is the Lambda's own `x-api-key` allowlist; the stage throttle is set
// in `deployOptions`.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;
const testnet = require('../../envs/testnet.json') as EnvironmentConfig;

function resourceTypes(config: EnvironmentConfig): string[] {
  const out = mkdtempSync(join(tmpdir(), 'usage-plan-'));
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
  ).Resources as Record<string, { Type: string }>;
  return Object.values(resources).map((r) => r.Type);
}

describe('API Gateway usage plan', () => {
  it.each([production, testnet])(
    'is absent in $envName, with the stage throttle kept',
    (config) => {
      const types = resourceTypes(config);
      expect(types).not.toContain('AWS::ApiGateway::UsagePlan');
      expect(types).not.toContain('AWS::ApiGateway::ApiKey');
      expect(types).not.toContain('AWS::ApiGateway::UsagePlanKey');
      expect(types).toContain('AWS::ApiGateway::Stage');
    }
  );
});
