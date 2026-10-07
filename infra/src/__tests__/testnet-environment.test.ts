import { mkdtempSync, readdirSync, readFileSync } from 'fs';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { beforeAll, describe, expect, it } from 'vitest';

import { createApp } from '../lib/app.js';
import { validateConfig, type EnvironmentConfig } from '../lib/types.js';

// Guards the testnet environment (task 0553): it reads SDF's public data lake,
// so it must deploy no VPC, ledger bucket or Galexie, ring the indexer on a
// schedule, alarm on a stall, and point every Lambda at the `testnet`
// database. Synthesised from envs/testnet.json without bundling the Rust
// Lambdas.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const testnet = require('../../envs/testnet.json') as EnvironmentConfig;

type Resource = { Type: string; Properties: Record<string, unknown> };
type Templates = Record<string, Record<string, Resource>>;

// Each synth writes into its own directory: CDK reads CDK_OUTDIR per app.
function synth(config: EnvironmentConfig): Templates {
  const out = mkdtempSync(join(tmpdir(), 'testnet-synth-'));
  process.env['CDK_OUTDIR'] = out;
  process.env['CDK_CONTEXT_JSON'] = JSON.stringify({
    'aws:cdk:bundling-stacks': [],
  });
  createApp({ config, cargoWorkspacePath: resolve(here, '../../..') });
  const templates: Templates = {};
  for (const file of readdirSync(out)) {
    const match = /^Explorer-testnet-(\w+)\.template\.json$/.exec(file);
    if (match?.[1]) {
      templates[match[1]] = JSON.parse(readFileSync(join(out, file), 'utf8'))
        .Resources as Record<string, Resource>;
    }
  }
  return templates;
}

// The reset runbook pauses the indexer (concurrency 0) and its last step
// resumes it (1); both states are synthesized, whatever the committed one is.
let templates: Templates = {};
let paused: Templates = {};
let resumed: Templates = {};
const ofType = (stack: string, type: string, from = templates): Resource[] =>
  Object.values(from[stack] ?? {}).filter((r) => r.Type === type);
// The indexer's consumer of its ingest queue (the enrichment worker has its own).
const ingestConsumers = (from: Templates): Resource[] =>
  ofType('Compute', 'AWS::Lambda::EventSourceMapping', from).filter((m) =>
    JSON.stringify(m.Properties['EventSourceArn']).includes('LedgerIngestQueue')
  );

beforeAll(() => {
  templates = synth(testnet);
  paused = synth({ ...testnet, indexerLambdaConcurrency: 0 });
  resumed = synth({ ...testnet, indexerLambdaConcurrency: 1 });
});

describe('testnet environment', () => {
  it('deploys no Galexie, ledger bucket, VPC or ClickHouse DNS record', () => {
    expect(Object.keys(templates).sort()).toEqual([
      'ApiGateway',
      'CloudWatch',
      'Compute',
      'Delivery',
      'Observability',
    ]);
  });

  it('points every Lambda at the testnet database and the indexer at the lake', () => {
    const env: Record<string, Record<string, string>> = Object.fromEntries(
      ofType('Compute', 'AWS::Lambda::Function').map((f) => [
        f.Properties['FunctionName'] as string,
        (f.Properties['Environment'] as { Variables: Record<string, string> })
          .Variables,
      ])
    );
    for (const vars of Object.values(env)) {
      expect(vars['CLICKHOUSE_DATABASE']).toBe('testnet');
      expect(vars['STELLAR_NETWORK_PASSPHRASE']).toBe(
        'Test SDF Network ; September 2015'
      );
    }
    const indexer = env['testnet-soroban-explorer-indexer'];
    expect(indexer?.['BUCKET_NAME']).toBe('aws-public-blockchain');
    // The indexer paces itself through its own queue.
    expect(indexer?.['INGEST_QUEUE_URL']).toBeDefined();
    expect(indexer?.['PUBLIC_ARCHIVE_PREFIX']).toBe(
      testnet.publicArchivePrefix
    );
    expect(env['testnet-soroban-explorer-api']?.['PUBLIC_ARCHIVE_PREFIX']).toBe(
      testnet.publicArchivePrefix
    );
  });

  it('paused: no queue consumer, no keepalive', () => {
    expect(ingestConsumers(paused)).toHaveLength(0);
    const [keepalive] = ofType('Compute', 'AWS::Scheduler::Schedule', paused);
    expect(keepalive?.Properties['State']).toBe('DISABLED');
  });

  it('once resumed, wakes the indexer once a minute and publishes no S3 events', () => {
    const schedules = ofType('Compute', 'AWS::Scheduler::Schedule', resumed);
    expect(schedules).toHaveLength(1);
    expect(schedules[0]?.Properties['State']).toBe('ENABLED');
    expect(schedules[0]?.Properties['ScheduleExpression']).toBe(
      'rate(1 minute)'
    );
    expect(JSON.stringify(schedules[0]?.Properties['Target'])).toContain(
      'keepalive'
    );
    expect(ingestConsumers(resumed)).toHaveLength(1);
    expect(ofType('Compute', 'AWS::SNS::Topic', resumed)).toHaveLength(0);
  });

  it('alarms on a stall instead of on Galexie, and leaves the cost monitor to production', () => {
    const names = ofType('CloudWatch', 'AWS::CloudWatch::Alarm').map(
      (a) => a.Properties['AlarmName']
    );
    expect(names).toContain('testnet-ingestion-stall');
    expect(names).not.toContain('testnet-galexie-ingestion-lag');
    expect(names).not.toContain('testnet-galexie-ephemeral-storage');
    expect(ofType('CloudWatch', 'AWS::CE::AnomalyMonitor')).toHaveLength(0);
  });

  it('refuses a lake folder next to an own ledger bucket', () => {
    expect(() =>
      validateConfig({ ...testnet, ledgerSource: 'galexie' })
    ).toThrow(/publicArchivePrefix is read only with ledgerSource/);
  });
});
