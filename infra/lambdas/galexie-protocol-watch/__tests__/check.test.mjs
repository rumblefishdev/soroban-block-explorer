import { describe, expect, it } from 'vitest';

import { coreVersion, verdict } from '../check.mjs';

// A registry holding the images of the P29 timeline, shaped like ECR and
// Docker Hub serve them: a manifest list naming one linux/amd64 manifest,
// whose config carries STELLAR_CORE_VERSION.
function fakeRegistry(images) {
  const manifests = {};
  const blobs = {};
  for (const [reference, env] of Object.entries(images)) {
    manifests[reference] = {
      manifests: [
        {
          digest: `${reference}-amd64`,
          platform: { os: 'linux', architecture: 'amd64' },
        },
      ],
    };
    manifests[`${reference}-amd64`] = {
      config: { digest: `${reference}-config` },
    };
    blobs[`${reference}-config`] = { config: { Env: env } };
  }
  return {
    async manifest(reference) {
      if (!manifests[reference])
        throw new Error(`manifest unknown: ${reference}`);
      return manifests[reference];
    },
    async blob(digest) {
      return blobs[digest];
    },
  };
}

const registry = fakeRegistry({
  'sha256:galexie-28.0.1': [
    'PATH=/usr/bin',
    'STELLAR_CORE_VERSION=28.0.1-3001.abc.noble',
  ],
  'sha256:galexie-29.0.0': ['STELLAR_CORE_VERSION=29.0.0-3589.4eb833373.noble'],
  'sha256:self-built': ['PATH=/usr/bin'],
});

const core28 = await coreVersion(registry, 'sha256:galexie-28.0.1');
const core29 = await coreVersion(registry, 'sha256:galexie-29.0.0');
const hub29 = { tag: 'c927ffc', ...core29 };

describe('coreVersion', () => {
  it('reads the major from the linux/amd64 config, not from a tag', () => {
    expect(core28).toEqual({ version: '28.0.1-3001.abc.noble', major: 28 });
  });

  it('cannot determine an image the registry does not hold', async () => {
    await expect(coreVersion(registry, 'sha256:unknown')).rejects.toThrow(
      'manifest unknown'
    );
  });

  it('cannot determine an image without STELLAR_CORE_VERSION', async () => {
    await expect(coreVersion(registry, 'sha256:self-built')).rejects.toThrow(
      'cannot determine: sha256:self-built carries no STELLAR_CORE_VERSION'
    );
  });
});

describe('verdict, replaying protocol 29', () => {
  it('LAGGING from 2026-09-22: core 28, the network core supports 29', () => {
    const result = verdict({
      network: { current: 28, supported: 29 },
      ours: core28,
      newest: hub29,
    });

    expect(result.ok).toBe(false);
    expect(result.message).toContain('LAGGING');
    expect(result.message).toContain('the bump is possible today');
  });

  it('LAGGING before an image exists says so', () => {
    const result = verdict({
      network: { current: 28, supported: 29 },
      ours: core28,
      newest: { tag: '28.0.1', ...core28 },
    });

    expect(result.message).toContain('no image for protocol 29 yet');
  });

  it('BEHIND after the vote on 2026-10-01', () => {
    const result = verdict({
      network: { current: 29, supported: 29 },
      ours: core28,
      newest: hub29,
    });

    expect(result.ok).toBe(false);
    expect(result.message).toContain('BEHIND');
  });

  it('OK once 29.0.0 runs', () => {
    const result = verdict({
      network: { current: 29, supported: 29 },
      ours: core29,
      newest: hub29,
    });

    expect(result.ok).toBe(true);
  });

  it('a Docker Hub outage is reported, never turns LAGGING into OK', () => {
    const result = verdict({
      network: { current: 28, supported: 29 },
      ours: core28,
      newest: { error: 'HTTP 503' },
    });

    expect(result.ok).toBe(false);
    expect(result.message).toContain('Docker Hub could not be read: HTTP 503');
  });
});
