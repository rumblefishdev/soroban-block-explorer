---
id: '0280'
title: 'CI flake: utoipa-swagger-ui downloads Swagger UI zip at build time'
type: BUG
status: completed
related_adr: []
related_tasks: ['0243']
tags: ['priority-low', 'effort-small', 'layer-infra']
links: []
history:
  - date: 2026-06-08
    status: backlog
    who: claude
    note: 'Spawned from 0243 — the Rust CI job for PR #248 flaked on the swagger-ui asset download.'
  - date: 2026-07-23
    status: active
    who: karolkow
    note: >
      Activated. Verified premise live: utoipa-swagger-ui still "9" (lock 9.0.2),
      build.rs still downloads v5.17.14.zip from github per build; CI ci.yml:148-149
      still runs --features swagger-ui. Fix chosen: enable the crate's `vendored`
      feature — utoipa-swagger-ui-vendored 0.1.2 ships res/v5.17.14.zip (byte-identical
      version), build.rs takes the CARGO_FEATURE_VENDORED branch first → no network.
  - date: 2026-07-23
    status: completed
    who: karolkow
    note: >
      Fixed in commit 8cbc163d: added `vendored` to the workspace utoipa-swagger-ui
      dependency features (Cargo.toml +1, Cargo.lock +7 for utoipa-swagger-ui-vendored
      0.1.2). Verified: (a) offline build exits 0 and logs "using vendored Swagger UI"
      after a forced build-script re-run — proves zero network fetch; (b) CI cmd
      `clippy -p api --all-targets --features swagger-ui -- -D warnings` exits 0;
      (c) OpenAPI spec byte-identical (api-types gate green). Swept repo: no
      download-era workaround/reference to remove; no other build.rs downloads at
      build time; prod Lambda build uses default features (never fetched).
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

Chose **Vendored assets** (option 1, most preferred — hermetic, no network,
smallest diff). Rejected the mirror (`SWAGGER_UI_DOWNLOAD_URL` → S3) and
cache+retry options: both stay non-hermetic and add infra/config surface for
zero benefit over vendoring.

One-line change — add `vendored` to the workspace `utoipa-swagger-ui` feature set:

```toml
# Cargo.toml
-utoipa-swagger-ui = { version = "9", features = ["axum"] }
+utoipa-swagger-ui = { version = "9", features = ["axum", "vendored"] }
```

`Cargo.lock` gains `utoipa-swagger-ui-vendored 0.1.2` (a build-dependency that
embeds `res/v5.17.14.zip` via `include_bytes!`). `utoipa-swagger-ui`'s `build.rs`
checks `CARGO_FEATURE_VENDORED` **first** and returns the embedded bytes before
ever reaching the github download branch → no build-time network.

## Acceptance Criteria

- [x] `cargo clippy -p api --features swagger-ui` and the lambda build run with
      **no network fetch** — proven by an offline build (`--offline`) that exits 0
      and logs `using vendored Swagger UI` after a forced build-script re-run.
      (Lambda build uses default features — it never fetched in the first place.)
- [x] CI Rust job no longer flakes on `InvalidArchive("Could not find EOCD")` —
      the download branch of `build.rs` is unreachable with `vendored` on.
- [x] Swagger UI still renders at `/api-docs` — vendored bundle is the
      **byte-identical** v5.17.14 (`utoipa-swagger-ui-vendored 0.1.2` ships exactly
      the tag the download URL pointed at). Version unchanged.

## Implementation Notes

- Change committed in `8cbc163d` (Cargo.toml +1/−1, Cargo.lock +7).
- `vendored` sits on the **workspace-level** dependency declaration, so it applies
  everywhere `utoipa-swagger-ui` compiles (the api crate's optional `swagger-ui`
  feature). No per-crate follow-up needed.
- OpenAPI spec is unaffected — re-extracted `openapi.json` is semantically
  identical (the `vendored` feature only changes where the UI _asset_ comes from,
  not any schema/route), so the `API types freshness` CI gate stays green with no
  regen artifacts to stage.
- Repo swept for download-era cruft (`SWAGGER_UI_DOWNLOAD_URL`, mirror, retry,
  cache hacks, stale comments): **none existed** — nobody had built a workaround,
  so nothing to remove. Only references to the old download live in this task file.

## Design Decisions

### From Plan

1. **Vendored over mirror/retry**: the task listed vendoring as top preference;
   confirmed it is byte-identical (v5.17.14) so there is zero behavior change,
   and it is the only fully hermetic option.

### Emerged

2. **No CI/workflow edit**: the fix is entirely at the dependency layer. The
   flaky CI steps (`ci.yml:148-149`) are left untouched — they now build against
   the embedded bundle automatically. Confirmed the lambda job (`rust-lambda`)
   runs on default features and never enabled `swagger-ui`, so it was never a
   flake surface despite the original task title mentioning "lambda-build".
