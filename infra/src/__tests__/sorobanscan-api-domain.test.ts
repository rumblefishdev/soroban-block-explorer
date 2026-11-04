import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import { validateConfig, type EnvironmentConfig } from '../lib/types.js';

// The API's host on the sorobanscan.com zone (task 0662): one more custom
// domain beside the current one, only when both fields are set.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;
const bare: EnvironmentConfig = {
  ...production,
  sorobanscanApiDomainName: undefined,
  sorobanscanApiCertificateArn: undefined,
};

function apiDomainNames(config: EnvironmentConfig): string[] {
  const out = mkdtempSync(join(tmpdir(), 'sorobanscan-api-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({ config, cargoWorkspacePath: resolve(here, '../../..') });
  const resources = JSON.parse(
    readFileSync(
      join(out, 'Explorer-production-ApiGateway.template.json'),
      'utf8'
    )
  ).Resources as Record<
    string,
    { Type: string; Properties: Record<string, unknown> }
  >;
  return Object.values(resources)
    .filter((r) => r.Type === 'AWS::ApiGateway::DomainName')
    .map((r) => r.Properties['DomainName'] as string)
    .sort();
}

describe('sorobanscan.com API domain', () => {
  it('serves production on both hosts, as the env file sets it', () => {
    expect(apiDomainNames(production)).toEqual([
      production.cloudflareApiDomainName,
      'api.sorobanscan.com',
    ]);
  });

  it('adds nothing while both fields are unset', () => {
    expect(apiDomainNames(bare)).toEqual([production.cloudflareApiDomainName]);
  });

  it('rejects a name without a certificate', () => {
    expect(() =>
      validateConfig({
        ...bare,
        sorobanscanApiDomainName: 'api.sorobanscan.com',
      })
    ).toThrow(/set together or not at all/);
  });
});
