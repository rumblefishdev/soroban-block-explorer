import { mkdtempSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import { stellarXdrMajor } from '../lib/stellar-xdr-pin.js';
import type { EnvironmentConfig } from '../lib/types.js';

// The protocol watch reads the stellar-xdr major from the indexer it watches
// (lore-0646): the Compute stack must set it from Cargo.toml, and the watch in
// the CloudWatch stack must name that function and be allowed to read it.
// Synthesised without bundling the Rust Lambdas.
const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '../../..');
const require = createRequire(import.meta.url);
const production = require('../../envs/production.json') as EnvironmentConfig;

type Resource = { Type: string; Properties: Record<string, unknown> };
type Template = {
  Resources: Record<string, Resource>;
  Outputs?: Record<string, { Value: unknown; Export?: { Name: string } }>;
};

function synth(): Record<string, Template> {
  const out = mkdtempSync(join(tmpdir(), 'protocol-watch-synth-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({ config: production, cargoWorkspacePath: repoRoot });
  const read = (stack: string) =>
    JSON.parse(
      readFileSync(
        join(out, `Explorer-production-${stack}.template.json`),
        'utf8'
      )
    ) as Template;
  return { Compute: read('Compute'), CloudWatch: read('CloudWatch') };
}

function lambdaNamed(template: Template, name: string): Resource {
  const fn = Object.values(template.Resources).find(
    (r) =>
      r.Type === 'AWS::Lambda::Function' &&
      r.Properties['FunctionName'] === name
  );
  if (!fn) throw new Error(`no Lambda ${name}`);
  return fn;
}

const variables = (fn: Resource) =>
  (fn.Properties['Environment'] as { Variables: Record<string, string> })
    .Variables;

describe('protocol watch: stellar-xdr', () => {
  const templates = synth();

  it('the indexer carries the workspace stellar-xdr major', () => {
    const indexer = lambdaNamed(
      templates.Compute,
      'production-soroban-explorer-indexer'
    );
    expect(variables(indexer)['STELLAR_XDR_MAJOR']).toBe(
      String(stellarXdrMajor(repoRoot))
    );
  });

  it('the watch names the indexer and may read only its configuration', () => {
    // The indexer's name reaches the CloudWatch stack as the Compute stack's
    // existing export of it, never as a copied literal.
    const [indexerId] = Object.entries(templates.Compute.Resources).find(
      ([, r]) =>
        r.Properties['FunctionName'] === 'production-soroban-explorer-indexer'
    ) ?? [''];
    const exported = Object.values(templates.Compute.Outputs ?? {}).find(
      (o) => JSON.stringify(o.Value) === JSON.stringify({ Ref: indexerId })
    );
    expect(exported?.Export?.Name).toBeTruthy();
    const imported = { 'Fn::ImportValue': exported?.Export?.Name };

    const watch = lambdaNamed(
      templates.CloudWatch,
      'production-galexie-protocol-watch'
    );
    expect(variables(watch)['INDEXER_FUNCTION_NAME']).toEqual(imported);

    const statements = Object.values(templates.CloudWatch.Resources)
      .filter((r) => r.Type === 'AWS::IAM::Policy')
      .flatMap(
        (r) =>
          (
            r.Properties['PolicyDocument'] as {
              Statement: { Action: string | string[]; Resource: unknown }[];
            }
          ).Statement
      )
      .filter((s) => s.Action === 'lambda:GetFunctionConfiguration');
    expect(statements).toHaveLength(1);
    const resource = JSON.stringify(statements[0]?.Resource);
    expect(resource).toContain(':function:');
    expect(resource).toContain(JSON.stringify(imported));
  });
});
