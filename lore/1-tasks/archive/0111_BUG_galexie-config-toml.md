---
id: '0111'
title: 'Fix Galexie container config: generate config.toml and set append command'
type: BUG
status: completed
related_adr: []
related_tasks: ['0034', '0108']
tags: [priority-high, effort-small, layer-infra]
milestone: 1
links: []
history:
  - date: 2026-04-08
    status: backlog
    who: fmazur
    note: 'Task created — Galexie exits with --help because no command or config.toml is provided'
  - date: 2026-04-08
    status: active
    who: fmazur
    note: 'Activated task'
  - date: 2026-04-08
    status: completed
    who: fmazur
    note: >
      Replaced non-functional env vars with inline config.toml generation.
      Live container runs append, backfill runs scan-and-fill.
      1 file changed (ingestion-stack.ts). Lint + build passing.
---

# Fix Galexie container config: generate config.toml and set append command

## Summary

Galexie ECS tasks exit immediately because (1) the default Docker CMD is `--help` and no subcommand is set, and (2) Galexie reads configuration from a TOML file, not environment variables. The current `sharedEnvironment` vars (`NETWORK_PASSPHRASE`, `DESTINATION`, `STELLAR_CORE_BINARY_PATH`) are silently ignored.

## Context

Galexie CLI accepts only three env vars: `START`, `END`, `CONFIG_FILE`. Everything else (datastore type, S3 bucket, network, schema) must be in a TOML config file. The container has `readonlyRootFilesystem: true` but `/tmp` is a writable mount.

Discovered during first staging deploy — container pulled successfully but exited with help text.

## Implementation Plan

### Step 1: Replace sharedEnvironment with config.toml generation

Replace the non-functional env vars with an entrypoint override that writes `config.toml` to `/tmp` (writable mount) and then execs Galexie. In `ingestion-stack.ts`:

- Remove `NETWORK_PASSPHRASE`, `DESTINATION`, `STELLAR_CORE_BINARY_PATH` from `sharedEnvironment`
- Keep `START`, `END` (these are valid Galexie env vars)
- Add `CONFIG_FILE: '/tmp/config.toml'` to environment
- Set `entryPoint` to `['/bin/bash', '-c']`
- Set `command` to a shell script that:
  1. Writes config.toml to `/tmp/config.toml`
  2. Execs `stellar-galexie append` (live) or `stellar-galexie scan-and-fill` (backfill)

Config template (live, pubnet):

```toml
[datastore_config]
type = "S3"

[datastore_config.params]
destination_bucket_path = "<bucket-name>/"
region = "<region>"

[datastore_config.schema]
ledgers_per_file = 1
files_per_partition = 64000

[stellar_core_config]
network = "pubnet"
```

### Step 2: Update backfill container

Same pattern but command uses `scan-and-fill` instead of `append`. `START`/`END` env vars are already set and work correctly.

### Step 3: Map network passphrase to Galexie network name

Galexie uses `network = "pubnet"` / `"testnet"` instead of the full passphrase string. Derive it from `stellarNetworkPassphrase` in CDK code with a simple mapping:

```typescript
const galexieNetwork = config.stellarNetworkPassphrase.includes('Public Global')
  ? 'pubnet'
  : 'testnet';
```

No changes to `EnvironmentConfig` or env JSON files needed.

## Acceptance Criteria

- [x] Galexie live container starts and runs `append` subcommand
- [x] Galexie reads config from generated `/tmp/config.toml`
- [x] S3 destination bucket configured correctly from CDK props
- [x] Network (pubnet/testnet) matches environment config
- [x] `sharedEnvironment` no longer contains ignored env vars
- [x] Backfill container uses `scan-and-fill` subcommand
- [x] Build passes
