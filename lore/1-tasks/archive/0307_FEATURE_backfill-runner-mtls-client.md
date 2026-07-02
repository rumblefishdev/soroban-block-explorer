---
id: '0307'
title: 'FEATURE: backfill-runner file-based mTLS client'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0306', '0283', '0231', '0240']
tags: [clickhouse, backfill-runner, mtls, effort-small, milestone-2]
milestone: 2
links: []
history:
  - date: '2026-06-19'
    status: active
    who: stkrolikiewicz
    note: >
      Spawned to enable the 0306 NFT pipeline run via mTLS (option A). The
      operator chose mTLS (client cert CN `stkrolikiewicz` → `dev_shared` admin,
      already issued + CN-mapped in Caddy, verified `SELECT currentUser()` →
      dev_shared) over the on-box plain-HTTP `default` + `CLICKHOUSE_PASSWORD`
      path that 0306 assumes. `backfill-runner` only builds a plain-HTTP client
      today, so it needs a file-based mTLS path.
  - date: '2026-06-25'
    status: completed
    who: claude
    note: >
      Completed + archived (status hygiene — left active). Code done +
      clippy-green (the [x] boxes). The only open gate — the live mTLS
      connect smoke — was exercised during the 0306 NFT pipeline run on
      2026-06-22: runner connected to https://ch.sorobanscan.rumblefish.dev
      as dev_shared (CN `stkrolikiewicz` → dev_shared via Caddy). Operator
      confirmed; box was just not checked off.
---

# FEATURE: backfill-runner file-based mTLS client

## Summary

Give `backfill-runner` a file-based mTLS option so it can reach the Caddy-fronted
prod ClickHouse (`https://ch.sorobanscan.rumblefish.dev`) as `dev_shared`
(admin / DDL), instead of the plain-HTTP `localhost:8123` + `default` password
path. The reusable `client_with_mtls()` builder already exists
(`crates/db-clickhouse/src/mtls.rs`); it just needs the `aws-mtls` feature
enabled on the runner and wiring into the sink builder.

## Context

- `backfill-runner build_sink` (`crates/backfill-runner/src/main.rs`) builds the CH
  sink with the plain `db_clickhouse::client(&cfg)` (HTTP, user+password). No mTLS.
- `db_clickhouse::mtls::client_with_mtls(host, &MtlsBundle, db)` builds a
  `clickhouse::Client` over rustls/hyper mTLS — but the whole `mtls` module is
  gated behind the `aws-mtls` feature, which also pulls `reqwest` + the
  Lambda-only `fetch_bundle_from_extension` / `client_from_lambda_env`.
- The 0306 pipeline (`nft-reparse` → `contract-type-rebuild` → `nft-reclassify`)
  runs through `backfill-runner` and needs write/DDL on prod CH. mTLS as
  `dev_shared` (task 0240 per-service users; admin profile, `allow_ddl=1`) is the
  chosen auth path.

## Implementation

Done in `backfill-runner` only (no `db-clickhouse` change):

1. **`Cargo.toml`** — `db-clickhouse = { path = "...", features = ["aws-mtls"] }`.
   The existing `aws-mtls` feature already exposes the reusable
   `client_with_mtls` builder; the Lambda-only `fetch_bundle_from_extension` /
   `client_from_lambda_env` compile unused here (pub fns → no dead-code warning).
2. **`main.rs`** — CLI flags `--ch-cert` / `--ch-key` / `--ch-ca` (env
   `CLICKHOUSE_CERT` / `_KEY` / `_CA`, `Option<PathBuf>`). In `build_sink`, when
   all three are `Some`, read the PEMs into an `MtlsBundle` and call
   `client_with_mtls(domain, &bundle, &cfg.database)` (domain = `cfg.url` with
   scheme + trailing slash stripped, since the builder prepends `https://`);
   all three `None` → the plain `db_clickhouse::client(&cfg)` (unchanged
   default); partial → panic ("all together or all omitted").

## Acceptance Criteria

- [x] `backfill-runner` builds with `db-clickhouse` `aws-mtls`;
      `--ch-cert/--ch-key/--ch-ca` flags present; all absent → plain client
      (no behaviour change for existing runs).
- [x] `cargo clippy -p backfill-runner --all-targets` green (12s, no warnings).
- [x] Smoke: runner with the cert connects to `https://ch.sorobanscan.rumblefish.dev`
      as `dev_shared` (e.g. a `--dry-run` read) — verified during the 0306 pipeline
      run 2026-06-22 (`currentUser()` → dev_shared).
- [x] **Docs updated** — N/A (no architecture-shape change; CLI flag only).
- [x] **API types regenerated** — N/A (no `crates/api/**` / `Cargo` workspace
      DTO change).

## Implementation Notes

- 2 files changed (`backfill-runner/Cargo.toml`, `backfill-runner/src/main.rs`);
  no `db-clickhouse` change. `cargo clippy --all-targets` clean in 12 s.
- Runner connects with: `--clickhouse-url https://ch.sorobanscan.rumblefish.dev
--ch-cert <cert.pem> --ch-key <key.pem> --ch-ca <ca.pem>` (cert/key/ca scp'd
  to the box; the `stkrolikiewicz` CN maps to `dev_shared` in Caddy).

## Design Decisions

### Emerged

1. **Enabled the existing `aws-mtls` feature instead of splitting a generic
   `mtls` feature** (the original plan). Lower risk + smaller diff: `reqwest`
   is already a workspace dep, and the Lambda-only extension fetch compiles
   unused (pub fns → no dead-code warning). A clean `mtls`-feature split
   (separating `client_with_mtls` from the AWS-extension fetch so the runner
   doesn't pull `reqwest`) is a possible follow-up if a reviewer prefers it.
2. **Domain extraction** — `client_with_mtls` prepends `https://`, so
   `build_sink` strips any `http(s)://` + trailing `/` from `cfg.url` first.
   Lets the operator set `CLICKHOUSE_URL=https://ch.sorobanscan…` naturally.
3. **Panic posture kept** — partial cert flags / unreadable PEM / build failure
   all `panic!`, matching `build_sink`'s existing debug-first stance (task 0145).
