import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import { validateConfig, type EnvironmentConfig } from '../lib/types.js';

// The paid routes under /x402/ (task 0682): testnet sells, for testnet USDC,
// behind a request limit of their own; production sells nothing yet.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;
const testnet = require('../../envs/testnet.json') as EnvironmentConfig;

type Resource = { Type: string; Properties: Record<string, unknown> };

function stack(config: EnvironmentConfig, name: string): Resource[] {
  const out = mkdtempSync(join(tmpdir(), 'x402-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({ config, cargoWorkspacePath: resolve(here, '../../..') });
  const template = readFileSync(
    join(out, `Explorer-${config.envName}-${name}.template.json`),
    'utf8'
  );
  return Object.values(JSON.parse(template).Resources as Resource[]);
}

const pathParts = (resources: Resource[]) =>
  resources
    .filter((r) => r.Type === 'AWS::ApiGateway::Resource')
    .map((r) => r.Properties['PathPart']);

const methodSettings = (resources: Resource[]) =>
  resources
    .filter((r) => r.Type === 'AWS::ApiGateway::Stage')
    .flatMap(
      (r) => r.Properties['MethodSettings'] as Record<string, unknown>[]
    );

const apiLambdaEnv = (resources: Resource[]) =>
  resources
    .filter((r) => r.Type === 'AWS::Lambda::Function')
    .map(
      (r) =>
        (r.Properties['Environment'] as { Variables?: Record<string, unknown> })
          ?.Variables ?? {}
    )
    .find((variables) => 'API_BASE_URL' in variables);

describe('paid routes under /x402/', () => {
  it('testnet maps them with their own request limit and tells the Lambda who is paid', () => {
    const gateway = stack(testnet, 'ApiGateway');
    expect(pathParts(gateway)).toContain('x402');
    expect(methodSettings(gateway)).toContainEqual(
      expect.objectContaining({
        // CloudFormation's spelling of `/x402/{proxy+}`.
        ResourcePath: '/~1x402~1{proxy+}',
        HttpMethod: 'GET',
        ThrottlingRateLimit: 10,
        ThrottlingBurstLimit: 20,
      })
    );
    // The rest of the API keeps its own limit.
    expect(methodSettings(gateway)).toContainEqual(
      expect.objectContaining({
        ResourcePath: '/*',
        HttpMethod: '*',
        ThrottlingRateLimit: testnet.apiGatewayThrottleRate,
      })
    );
    expect(apiLambdaEnv(stack(testnet, 'Compute'))).toMatchObject({
      X402_PAY_TO: testnet.x402?.payTo,
      X402_NETWORK: 'stellar:testnet',
      X402_PRICE_USD: '0.001',
    });
  });

  it('production sells nothing', () => {
    expect(production.x402).toBeUndefined();
    expect(pathParts(stack(production, 'ApiGateway'))).not.toContain('x402');
    expect(apiLambdaEnv(stack(production, 'Compute'))).not.toHaveProperty(
      'X402_PAY_TO'
    );
  });

  it('refuses to sell one network’s data for the other’s USDC', () => {
    const mainnetDataForTestnetUsdc = { ...production, x402: testnet.x402 };
    expect(() => validateConfig(mainnetDataForTestnetUsdc)).toThrow(
      /x402\.network must be "stellar:pubnet"/
    );
  });
});
