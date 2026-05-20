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
   `media_url`, `collection_name` on the resulting `nfts` row.

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

- `crates/enrichment-shared/tests/sep1_real_issuer_smoke.rs` —
  `#[ignore] #[tokio::test] async fn sep1_real_issuer_smoke()` that:
  - Picks a known stable issuer (suggestion: `ultrastellar.com` or a
    similarly long-running anchor; final choice during implementation
    after a stability sanity check on a few candidates).
  - Calls the full `enrich_and_persist::sep1_assets::enrich_asset`
    (canonical entry point; the example in Step 2 uses the same symbol —
    if the public function is renamed during implementation, both
    references update together).
  - Asserts non-NULL `assets.icon_url` and `assets.name` on the resulting
    row.
- `crates/enrichment-shared/tests/nft_token_uri_real_collection_smoke.rs`
  — `#[ignore] #[tokio::test] async fn nft_metadata_real_collection_smoke()`
  that:
  - Picks a known stable Soroban NFT collection (suggestion: a long-running
    public collection with stable JSON-metadata behind IPFS; final choice
    during implementation).
  - Calls the full `enrich_and_persist::nft_token_uri::enrich_nft_token_uri`.
  - Asserts non-NULL `nfts.name`, `nfts.media_url`, `nfts.collection_name`.
  - Asserts `media_url` starts with `https://` (verifies the ipfs→https
    gateway resolver shipped post-0196 merge is wired correctly).
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

### Step 2 — write the two test functions

Each follows the same pattern:

```rust
#[ignore]
#[tokio::test]
async fn sep1_real_issuer_smoke() {
    let pool = test_pool().await;
    let asset_id = insert_test_asset(&pool, "USDC", "GA5ZSE..." /* ultra issuer */).await;
    let fetcher = Sep1Fetcher::new_default();
    enrich_and_persist::sep1_assets::enrich_asset(&pool, asset_id, &fetcher)
        .await
        .expect("live SEP-1 fetch + persist");
    let row = sqlx::query!("SELECT icon_url, name FROM assets WHERE id = $1", asset_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(row.icon_url.is_some(), "icon_url must be populated");
    assert!(row.name.is_some(), "name must be populated");
}
```

Mirror shape for NFT.

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

- [ ] Two `#[ignore]` test files committed under
      `crates/enrichment-shared/tests/`.
- [ ] Tests pass against the chosen fixture issuer / collection at PR time
      (run output pasted in PR description).
- [ ] `cargo test -p enrichment-shared` (without `--ignored`) stays
      hermetic — no new tests run by default.
- [ ] `.github/workflows/live-smoke.yml` workflow-dispatch trigger works.
- [ ] Fixture choice + rotation policy documented in each test file
      header.
- [ ] One-line cross-reference added to
      `docs/architecture/indexing-pipeline/enrichment.md` (or current
      successor doc).

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
