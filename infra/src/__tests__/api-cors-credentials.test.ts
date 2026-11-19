import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import type { EnvironmentConfig } from '../lib/types.js';

// The free-tier session is an HttpOnly cookie on the API host (task 0677).
// The SPA's mint is a JSON POST, so the browser sends a CORS preflight first,
// which API Gateway answers itself; without `Allow-Credentials: true` there
// the browser refuses the credentialed mint and no session is ever set.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;
const testnet = require('../../envs/testnet.json') as EnvironmentConfig;

type Resource = { Type: string; Properties: Record<string, unknown> };

function preflightHeaders(config: EnvironmentConfig): Record<string, string>[] {
  const out = mkdtempSync(join(tmpdir(), 'api-cors-'));
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
  ).Resources as Record<string, Resource>;
  return Object.values(resources)
    .filter(
      (r) =>
        r.Type === 'AWS::ApiGateway::Method' &&
        r.Properties['HttpMethod'] === 'OPTIONS'
    )
    .map((r) => {
      const integration = r.Properties['Integration'] as {
        IntegrationResponses: { ResponseParameters: Record<string, string> }[];
      };
      return integration.IntegrationResponses[0].ResponseParameters;
    });
}

describe('API Gateway CORS preflight', () => {
  it.each([production, testnet])('allows credentials in $envName', (config) => {
    const preflights = preflightHeaders(config);
    expect(preflights.length).toBeGreaterThan(0);
    for (const headers of preflights) {
      expect(
        headers['method.response.header.Access-Control-Allow-Credentials']
      ).toBe("'true'");
    }
  });
});
