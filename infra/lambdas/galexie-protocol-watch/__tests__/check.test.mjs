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
        throw new Error(`cannot determine: ${reference} is not in ECR`);
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
      'cannot determine'
    );
  });

  it('cannot determine an image without STELLAR_CORE_VERSION', async () => {
    await expect(coreVersion(registry, 'sha256:self-built')).rejects.toThrow(
      'cannot determine: sha256:self-built carries no STELLAR_CORE_VERSION'
    );
  });
});

describe('verdict, replaying protocol 29', () => {
  it('until 2026-09-24 Docker Hub has nothing newer: OK', () => {
    const result = verdict({
      ours: core28,
      newest: { tag: '2aa7c4a', ...core28 },
    });

    expect(result.ok).toBe(true);
  });

  it('from 2026-09-24 commit tag c927ffc carries core 29: deploy it', () => {
    const result = verdict({ ours: core28, newest: hub29 });

    expect(result.ok).toBe(false);
    expect(result.message).toContain('a Galexie with core 29 is out');
    expect(result.message).toContain('Docker Hub tag c927ffc');
  });

  it('OK once 29.0.0 runs', () => {
    const result = verdict({ ours: core29, newest: hub29 });

    expect(result.ok).toBe(true);
  });

  it('OK when ours is newer than Docker Hub (a self-built image)', () => {
    const result = verdict({
      ours: core29,
      newest: { tag: '28.0.1', ...core28 },
    });

    expect(result.ok).toBe(true);
  });
});
