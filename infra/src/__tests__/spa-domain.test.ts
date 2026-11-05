import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import { validateConfig, type EnvironmentConfig } from '../lib/types.js';

// The SPA on sorobanscan.com with its old host still served (task 0663): both
// names on the distribution, Route 53 records for the old one only, and the
// API's CORS open to both.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;

type Resource = { Type: string; Properties: Record<string, unknown> };

function synth(stack: string): Resource[] {
  const out = mkdtempSync(join(tmpdir(), 'spa-domain-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({
    config: production,
    cargoWorkspacePath: resolve(here, '../../..'),
  });
  const template = JSON.parse(
    readFileSync(
      join(out, `Explorer-production-${stack}.template.json`),
      'utf8'
    )
  ) as { Resources: Record<string, Resource> };
  return Object.values(template.Resources);
}

describe('SPA on sorobanscan.com', () => {
  it('serves both hosts and keeps Route 53 records for the old one only', () => {
    const delivery = synth('Delivery');
    const config = delivery.find(
      (r) => r.Type === 'AWS::CloudFront::Distribution'
    )?.Properties['DistributionConfig'] as { Aliases: string[] } | undefined;
    expect(config?.Aliases).toEqual([
      'sorobanscan.com',
      'sorobanscan.rumblefish.dev',
    ]);
    const records = delivery
      .filter((r) => r.Type === 'AWS::Route53::RecordSet')
      .map((r) => `${r.Properties['Type']} ${r.Properties['Name']}`)
      .sort();
    expect(records).toEqual([
      'A sorobanscan.rumblefish.dev.',
      'AAAA sorobanscan.rumblefish.dev.',
    ]);
  });

  it('opens the API to both SPA origins', () => {
    const env = synth('Compute')
      .filter((r) => r.Type === 'AWS::Lambda::Function')
      .map(
        (r) =>
          (
            r.Properties['Environment'] as {
              Variables?: Record<string, string>;
            }
          )?.Variables?.['CORS_ALLOW_ORIGIN']
      )
      .find(Boolean);
    expect(env).toBe(
      'https://sorobanscan.com,https://sorobanscan.rumblefish.dev'
    );
  });

  it('rejects two SPA names inside the hosted zone', () => {
    expect(() =>
      validateConfig({
        ...production,
        domainName: 'sorobanscan.rumblefish.dev',
        legacyDomainName: 'www.sorobanscan.rumblefish.dev',
      })
    ).toThrow(/exactly one of domainName/);
  });
});
