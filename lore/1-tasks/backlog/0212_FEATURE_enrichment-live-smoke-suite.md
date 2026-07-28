---
id: '0212'
title: 'FEATURE: enrichment live-smoke suite — #[ignore] integration tests against real issuers / collections'
type: FEATURE
status: backlog
related_adr: ['0043']
related_tasks: ['0188', '0191', '0195', '0197']
tags:
  [
    priority-medium,
    effort-small,
    layer-tests,
    layer-enrichment,
    smoke-tests,
    ci-opt-in,
  ]
milestone: 3
links:
  - https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0001.md
history:
  - date: '2026-05-12'
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0197 audit-prep punch list. Bundles two deferred
      `#[ignore]` integration test items that share the same shape
      (live HTTP/IPFS round trip against a known real-world source):
      0188 SEP-1 fetcher real-issuer smoke (`assets.description`,
      `home_page`, `icon_url` end-to-end against e.g. ultrastellar.com),
      and 0195 §2d NFT metadata smoke (non-NULL `name` / `media_url` /
      `collection_name` against a known JSON-metadata collection).
      Both originally deferred at PR time; bundled here because the
      test infrastructure (`#[ignore]` opt-in, real network egress,
      fixture-asset taxonomy) is identical.
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Re-scoped after measurement — the tests exist, the gate does not.**
      Checked during a sweep of the 100 open tasks, where this one first looked
      closable. It is not: reading the acceptance criteria instead of the
      summary changed the verdict.
      DONE: four smoke tests are committed and cover both halves the summary
      asks for — `smoke_real_sep1_resolves_icon_and_name`,
      `smoke_ch_sep1_real_and_sentinel` (SEP-1) and
      `smoke_ch_nft_real_and_sentinel`,
      `live_mainnet_zero_arg_token_uri_success` (NFT). All carry `#[ignore]`, so
      the default `cargo test` stays hermetic. They sit inline in `src/` rather
      than in a `tests/` directory (which does not exist) — accepted as-is.
      MISSING, and the reason this stays open: **`.github/workflows/live-smoke.yml`
      does not exist**, so nothing ever triggers them. The task's own framing is
      "a recurring gate, unlike 0197's one-time snapshot" — without the workflow
      there is no gate, only tests a human may remember to run. Also absent: the
      fixture rotation policy (which the Notes call an acceptance gate, not
      future work) and the cross-reference in
      `docs/architecture/indexing-pipeline/enrichment.md`.
      Remaining effort is small and entirely CI + docs; no test-writing left.
---

# FEATURE: enrichment live-smoke suite — `#[ignore]` integration tests against real issuers / collections

## Summary

Add two `#[ignore]` integration tests that exercise the live enrichment paths
end-to-end against real-world data:

1. **SEP-1 smoke** — Fetch a known live issuer's stellar.toml, run the full
   `enrich_and_persist::sep1_assets` flow, assert non-NULL `icon_url` and
   `name` on the resulting `assets` row.
2. **NFT metadata smoke** — Fetch a known live NFT collection's
   `token_uri()` metadata, run the full
   `enrich_and_persist::nft_token_uri` flow, assert non-NULL `name`,
   `media_url` on the resulting `nfts` row. **`collection_name` does NOT
   come from `token_uri()` JSON** (corrected by task 0340 — no real
   Stellar NFT emits a JSON `"collection"` field, 0/68 on prod); it is
   sourced from the contract-level SEP-50 `name()` RPC, so a
   `collection_name` assertion belongs against a collection known to
   export `name()`, not a "JSON-metadata collection".

Both run only when CI is explicitly invoked with `cargo test -- --ignored`.
Goal: catch regressions where the live worker silently breaks against real
issuers (e.g. SEP-1 TOML schema drift, IPFS gateway changes, ipfs:// URI
resolution bug). Default `cargo test` stays hermetic.

## Context

### Why deferred originally

Both items were listed as `[ ]` deferred-to-PR-time in their parent tasks:

- **0188 Future Work** — "Real-issuer `#[ignore]` smoke test against e.g.
  `ultrastellar.com` — verifies HTTP path end-to-end with a real network
  round-trip." The 0188 fixture-server tests cover the wire format, but
  not behaviour against a real issuer's TOML quirks.
- **0195 AC §2d** — "Sample query: non-NULL `name` / `media_url` /
  `collection_name` on minted NFTs from a known JSON-metadata collection."
  Deferred because at 0195 ship time no canonical "known-good" collection
  had been picked.

### Why bundled (not two separate tasks)

Identical shape:

- One Rust test file per kind (`crates/enrichment-shared/tests/`),
  `#[ignore]` annotation, real-network egress, asserts on persisted DB
  columns.
- Same fixture-asset taxonomy challenge: pick one stable issuer + one
  stable NFT collection that won't flap CI when their team rotates
  TOML / metadata.
- Same CI integration: `cargo test -- --ignored` opt-in. Plus a
  workflow-dispatch trigger so anyone can manually run "live smoke"
  from GitHub Actions.

Two micro-tasks would duplicate the infra discussion. One task lands the
fixture-asset taxonomy + CI opt-in pattern + both kinds together.

### Why not in 0197

0197 is audit + docs, not test code. The audit _will_ surface "is the
enrichment path actually wired end-to-end" as a question, and 0197 Step 1
includes a manual one-time smoke check during audit run-time. This task
turns that one-time manual check into a persistent `#[ignore]` test so
the regression risk doesn't snap back.

## Scope

### In

- ~~SEP-1 smoke test~~ — **DONE.** Landed inline in
  `crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs`, not in a
  `tests/` directory (none exists). Entry point is
  `enrich_and_persist::sep1_assets::enrich_asset_from_sep1(client: &Client, …)`
  — renamed from the `enrich_asset` this bullet used to name, and taking a
  `clickhouse::Client` rather than a pool. Assertions read
  `asset_enrichment.icon_url` / `.name`, not `assets.*`: `assets.name` was
  dropped in 0304 (`crates/db-clickhouse/schema/init.sql:301`) and
  `assets.icon_url` is marked dead in favour of `asset_enrichment`. Fixture
  issuer is USDC (`GA5ZSEJY…`, home domain centre.io) rather than the
  ultrastellar.com suggested here. See the Step 2 table for exact locations.
- ~~NFT metadata smoke test~~ — **DONE.** Landed inline in
  `crates/enrichment-shared/src/enrich_and_persist/nft_token_uri.rs:358`
  (`smoke_ch_nft_real_and_sentinel`), plus
  `live_mainnet_zero_arg_token_uri_success` in `nft_token_uri/client.rs`. The
  `collection_name` caveat below still holds and is why it is not asserted
  non-NULL: per 0340 the value comes from the contract-level SEP-50 `name()`
  RPC, not from `token_uri()` JSON.
- Manual workflow-dispatch entry in `.github/workflows/` (e.g.
  `live-smoke.yml`) that runs `cargo test -p enrichment-shared --tests
-- --ignored`. Operator-driven, not on push.
- README note in the test file headers documenting the fixture asset /
  collection ID and the rotation policy (when to update if the live target
  goes offline).

### Out

- Periodic CI run (e.g. nightly schedule) — operator-driven for now.
  If/when CI catches a regression that the manual run could have caught
  earlier, schedule it. Not premature.
- Negative-path smokes (issuer with malformed TOML, NFT collection with
  broken metadata) — the existing fixture-server / unit tests cover these
  hermetically; a real-world malformed-path smoke is hard to keep stable.
- Adding new enrichment kinds. Bundle covers exactly the two kinds shipped
  today (sep1_assets, nft_token_uri). Future kinds add their own smoke as
  part of their delivery.

## Implementation Plan

### Step 1 — fixture asset / collection picks

Spend ~30 min looking at:

- Long-running anchor issuers on Stellar mainnet (Ultra Stellar, ClickPesa,
  AnchorUSD, MoneyGram — pick one with stable `stellar.toml` and stable
  on-chain footprint).
- Long-running Soroban NFT collections (look for collections with > 100
  mints and a stable `image` field inside the JSON metadata returned by
  `token_uri()`; verify the IPFS gateway resolution works for 3-5 random
  tokens before committing to the choice).

Document the choice + rotation policy in the test file headers.

### Step 2 — write the two test functions — **DONE**

Shipped, ClickHouse-native. The planning sketch that used to sit here was
Postgres-shaped in five ways at once — `sqlx::query!`, a `$1` bind placeholder,
a `test_pool()` handle, `FROM assets` for a column that now lives in
`asset_enrichment`, and an `enrich_asset` entry point since renamed — so it has
been replaced by pointers to what was actually committed. Do not resurrect it.

| Test                                      | Location                                                               |
| ----------------------------------------- | ---------------------------------------------------------------------- |
| `smoke_ch_sep1_real_and_sentinel`         | `crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs:414`   |
| `smoke_real_sep1_resolves_icon_and_name`  | `crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs:388`   |
| `smoke_ch_nft_real_and_sentinel`          | `crates/enrichment-shared/src/enrich_and_persist/nft_token_uri.rs:358` |
| `live_mainnet_zero_arg_token_uri_success` | `crates/enrichment-shared/src/nft_token_uri/client.rs`                 |

The shipped shape, for reference: `enrich_asset_from_sep1(client: &Client, …)`
takes a `clickhouse::Client`; readback is a `client.query("SELECT icon_url, name
FROM asset_enrichment FINAL WHERE …").bind(…).fetch_one::<Readback>()` against a
`#[derive(Row, Deserialize)]` struct. Each test carries an `#[ignore]` reason
naming what it needs (live local ClickHouse, network, mainnet Soroban-RPC), so
the default `cargo test` stays hermetic.

### Step 3 — GitHub Actions workflow

Workflow-dispatch only (no `on: push`). Targets `enrichment-shared` package.
Reports pass/fail with the asset/collection ID in the run summary so a
failure points immediately at "issuer rotated TOML" vs "code regression".

### Step 4 — docs

- README inside each test file documenting fixture choice + rotation
  policy.
- Append a one-line note to
  `docs/architecture/indexing-pipeline/enrichment.md` (or its successor):
  "Live-smoke regression tests live under
  `crates/enrichment-shared/tests/*_real_*.rs` and are opt-in via
  `cargo test -- --ignored` or the `live-smoke.yml` workflow-dispatch."

## Acceptance Criteria

> **Status 2026-07-22 (measured, not assumed).** The tests themselves exist —
> someone wrote them and never came back to the task. What is missing is the
> whole "recurring gate" half: without the workflow, these are tests a human
> can run, not a gate. See the history entry.

- [x] ~~Two `#[ignore]` test files committed under
      `crates/enrichment-shared/tests/`~~ — **done differently**: four smoke
      tests live _inline_ in `src/` (`enrich_and_persist/sep1_assets.rs`,
      `nft_token_uri.rs`), not in a `tests/` directory, which does not exist.
      `smoke_real_sep1_resolves_icon_and_name`, `smoke_ch_sep1_real_and_sentinel`,
      `smoke_ch_nft_real_and_sentinel`, `live_mainnet_zero_arg_token_uri_success`.
      Accepting the location as-is; splitting them out buys nothing.
- [ ] Tests pass against the chosen fixture issuer / collection at PR time
      (run output pasted in PR description) — **unverified**, they need network
      and a live ClickHouse, so nobody has evidence they still pass.
- [x] `cargo test -p enrichment-shared` (without `--ignored`) stays
      hermetic — no new tests run by default. All four carry `#[ignore]`.
- [ ] `.github/workflows/live-smoke.yml` workflow-dispatch trigger works —
      **the file does not exist**. This is the gap that keeps the task open:
      the point was a recurring gate, and there is nothing to trigger.
- [ ] Fixture choice + rotation policy documented in each test file
      header — partially: the tests carry `#[ignore = "..."]` reasons and a
      run command, but no "if this issuer rotates its TOML, fall back to X"
      policy, which the Notes below call an acceptance gate.
- [ ] One-line cross-reference added to
      `docs/architecture/indexing-pipeline/enrichment.md` (or current
      successor doc) — **absent**; the file exists but never mentions the smoke
      tests.

## Notes

- **Fixture-asset stability matters more than coverage breadth.** A
  flaky smoke that fails when the issuer rotates TOML is worse than no
  smoke at all (engineer-attention thrash). Step 1's 30-min stability
  spike is therefore part of the acceptance gate, not Future Work.
- **Why bundle and not add to 0197.** 0197 audit produces a one-time
  snapshot. This task produces a _recurring_ gate. Different lifetimes
  (one-shot doc vs evergreen test). 0197 still includes a one-time
  manual smoke during audit run (Step 1 sub-bullet) — that's the
  audit's job; this task's job is the regression-catch.
- **Rotation policy.** Each fixture choice carries a "if this stops
  working, fall back to <alternate> and update this file" comment.
  Owners (whoever ran the PR) noted in the file header.
