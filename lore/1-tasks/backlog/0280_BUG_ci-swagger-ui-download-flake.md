---
id: '0280'
title: 'CI flake: utoipa-swagger-ui downloads Swagger UI zip at build time'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0243']
tags: ['priority-low', 'effort-small', 'layer-infra']
links: []
history:
  - date: 2026-06-08
    status: backlog
    who: claude
    note: 'Spawned from 0243 — the Rust CI job for PR #248 flaked on the swagger-ui asset download.'
---

# CI flake: utoipa-swagger-ui downloads Swagger UI zip at build time

## Summary

The `cargo clippy -p api --features swagger-ui` / lambda-build CI steps depend on
`utoipa-swagger-ui`'s build script **fetching the Swagger UI archive from GitHub
on every build**. When GitHub returns a partial/throttled response the build
panics and the whole Rust check fails — a non-deterministic, code-unrelated red.

## Context

Surfaced on PR #248 (task 0243 LP-detail OOM fix). The Rust job failed with:

```
error: failed to run custom build command for `utoipa-swagger-ui v9.0.2`
  start download: .../swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip
  reqwest feature: Err(NotPresent)            # falls back to system `curl`
  panicked at build.rs:219:51:
  failed to open downloaded Swagger UI: InvalidArchive("Could not find EOCD")
```

`Could not find EOCD` = the downloaded zip is truncated (failed/throttled
download), not a bad pin. A plain re-run went green — confirming a transient
network flake, not a code defect. The build is non-hermetic: it reaches out to
github.com archive endpoints during compile.

## Implementation

Pick one (rough order of preference):

- **Vendored assets** — switch to the `vendored` feature / `utoipa-swagger-ui-vendored`
  crate so the Swagger UI bundle ships in the dependency, no build-time download.
  Hermetic, no network. Verify it covers the version we pin (v5.17.14).
- **Point `SWAGGER_UI_DOWNLOAD_URL` at our own cached/mirrored asset** (S3 / repo
  LFS / CI cache) so the build never hits github.com archive endpoints.
- **CI cache + retry** — cache the resolved `target/.../out/*.zip` across runs and
  wrap the download step in a retry. Cheapest, but still non-hermetic.

Confirm the chosen approach leaves `--features swagger-ui` working (prod API
serves swagger-ui — see `feat/0243-prod-ch-enable-and-swagger`).

## Acceptance Criteria

- [ ] `cargo clippy -p api --features swagger-ui` and the lambda build run with
      **no network fetch** of the Swagger UI archive (or a deterministic,
      retried, cached fetch).
- [ ] CI Rust job no longer flakes on `InvalidArchive("Could not find EOCD")`.
- [ ] Swagger UI still renders at the prod API `/api-docs` endpoint (version
      unchanged unless deliberately bumped).
