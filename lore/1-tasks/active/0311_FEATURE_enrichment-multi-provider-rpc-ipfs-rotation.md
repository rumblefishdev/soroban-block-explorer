---
id: '0311'
title: 'FEATURE: enrichment multi-provider RPC + IPFS gateway rotation/failover'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0231', '0301', '0306', '0307']
tags: [enrichment, nft, rpc, ipfs, effort-small, milestone-2]
milestone: 2
links: []
history:
  - date: '2026-06-22'
    status: active
    who: stkrolikiewicz
    note: >
      Spawned to lift the NFT enrichment 84% ceiling. The single hardcoded
      SDF RPC (`mainnet.sorobanrpc.com`) rate-limits (429) under the backfill
      burst, leaving ~1,450 hot NFTs un-enriched. Chosen lever: free
      multi-provider RPC rotation + failover (not self-host, not paid). A
      live sieve on-box found 4 keyless, in-sync mainnet RPCs and 2
      path-style IPFS gateways; the hardcoded `cloudflare-ipfs.com` gateway
      is dead/unreachable from the box. Validate locally + on-box batch
      before the PR.
---

# FEATURE: enrichment multi-provider RPC + IPFS gateway rotation/failover

## Summary

Give the NFT `token_uri` enrichment fetcher a **pool of Soroban RPC endpoints**
and a **pool of IPFS gateways**, with **round-robin + failover on transient
transport errors (429 / 5xx / timeout)**, instead of the single hardcoded SDF
RPC + single dead IPFS gateway. Goal: clear the ~1,450 RPC-429-throttled hot
NFTs (84% → ~95%+) for **$0** and no new infra.

## Status: Active

**Current state:** Sieve done (pools picked, below). Next: implement in
`enrichment-shared`, unit-test on mac, validate a small batch on-box, then PR.

## Context

- `NftTokenUriFetcher` holds a single `rpc_url` and hits `mainnet.sorobanrpc.com`
  (`crates/enrichment-shared/src/nft_token_uri/client.rs:35,54`). Under the 0306
  backfill burst (12,835 tokens) the SDF RPC per-second-limits → 429 → the run
  capped at 84% (10,760 with name+media; ~1,450 stuck on RPC-429). A
  concurrency-2 retry recovered only ~14 → the public box is a hard global wall.
- IPFS side: `IPFS_GATEWAY_BASE` is hardcoded to `cloudflare-ipfs.com`
  (`client.rs:36`) — Cloudflare sunset its public gateway; from the prod box it
  is `000` (unreachable). Every `ipfs://` URI that needs a gateway fails there.
- Decision (over self-host / paid RPC): **free multi-provider rotation**. Live
  sieve from the box confirmed enough keyless, in-sync endpoints.
- Consistency verified: RPC is a deterministic current-state read of (near-)
  immutable `token_uri`; IPFS is content-addressed (same CID → byte-identical).
  Empirically: all RPCs at the same ledger; both gateways returned the same
  51,383-byte file / same sha256. We take **first-success, not quorum** — fine
  for deterministic immutable data.

## Implementation Plan

All in `crates/enrichment-shared/src/nft_token_uri/client.rs` (call sites
`new()` in runner + worker inherit automatically; **no call-site change**).

### Step 1 — RPC pool + failover

- `rpc_url: Arc<String>` → `rpc_urls: Arc<Vec<String>>` + round-robin cursor
  (`AtomicUsize`).
- Wrap `simulate_token_uri_with_fallback` in a provider loop: **transient
  transport** (429 / 502-504 / connect/timeout) → next provider; **deterministic**
  (contract revert / arity / parse) → return (same on every provider).
- Round-robin **start offset per request** → proactively spread load (fewer 429s
  in the first place) + failover when one is down.
- `new()` reads env `SOROBAN_RPC_URLS` (comma-separated); fall back to single
  `SOROBAN_RPC_URL`; fall back to today's default. `with_rpc_url` kept for tests.

### Step 2 — IPFS gateway pool + dead-default fix

- `IPFS_GATEWAY_BASE` (single const) → gateway list; `resolve_ipfs_to_https`
  rotates + the metadata-fetch path fails over on transient.
- Replace dead `cloudflare-ipfs.com` with `[ipfs.io, gateway.pinata.cloud]`
  (path-style 200, box-reachable — see sieve). Env override (e.g.
  `IPFS_GATEWAY_BASES`) optional.
- Constraint: client uses `Policy::limited(0)` (no redirects, SSRF guard) →
  only **path-style 200** gateways work; subdomain-redirect ones (dweb.link,
  cloudflare) are out.

### Step 3 — robustness: 3xx no longer panics

- `simulate_transaction` + `fetch_uncached` do
  `resp.error_for_status().expect_err(...)` on any non-2xx; a **3xx** makes
  `error_for_status()` return `Ok` → **panic**. Handle 3xx as an `Http`/redirect
  error gracefully (relevant once gateways are in play).

### Step 4 — validate, then PR

- Unit tests (wiremock): failover advances on 429/5xx, stops on deterministic
  error, round-robin spreads, all-providers-fail surfaces last error.
- Docker-cross-build `enrich` → scp → on-box small batch (`--limit 100` of the
  un-enriched, `SOROBAN_RPC_URLS=…` + IPFS pool) → confirm real 429-recovery.
- Then full re-drain + PR with the recovered count.

## Acceptance Criteria

- [x] RPC pool + round-robin + failover-on-transient implemented; deterministic
      errors do **not** failover. (`simulate_with_failover`; `is_endpoint_fault`)
- [x] IPFS gateway pool + failover; `cloudflare-ipfs.com` replaced with
      `[ipfs.io, gateway.pinata.cloud]`. (`fetch_metadata_with_failover`,
      `ipfs_candidate_urls`, `DEFAULT_IPFS_GATEWAYS`)
- [x] 3xx response handled gracefully (no `expect_err` panic). (`non_success_error`
      → `HttpStatus`; `is_endpoint_fault` also covers `source.is_redirect()`)
- [x] Env-driven (`SOROBAN_RPC_URLS` / `IPFS_GATEWAY_BASES`); **back-compat**:
      unset → identical to today (single SDF RPC). Worker + runner inherit via
      `new()` (no call-site change — blast-radius grep confirmed).
- [x] Unit tests green (**75 pass / 0 fail**, +7 new); `cargo clippy
    -p enrichment-shared --all-targets` clean; rustfmt clean.
- [x] On-box small batch shows 429-recovery vs the single-RPC baseline.
      **VALIDATED 2026-06-22:** a `--retry-sentinels --limit 100 --concurrency 8`
      batch produced **0 RPC-429** (vs the single-RPC wall) — RPC errors are now
      contract-reverts (`Error(WasmVm, InvalidAction)`), i.e. the pool reaches the
      RPC fine. +13/100 recovered (nft name+media 10,774 → 10,787).
- [ ] ~~Full re-drain recovers the ~1,450 → ≥95%~~ **REVISED — RPC-429 was masking
      a permanent-dominated residual.** The first-100 sentinel sample split:
      ~27 contract-reverts (permanent), ~33 `data:`-URI (recoverable — see
      Findings), 18 IPFS-429/conn (transient → lower concurrency), ~6 dead
      links (permanent). True ceiling well below 95%; the recoverable levers are
      (a) IPFS concurrency 3-4 and (b) `data:`-URI support (separate follow-up).

## Validation Findings (2026-06-22)

1. **RPC rotation works** — 0 RPC-429 in the batch log; the 4-provider pool
   eliminated the single-RPC wall (the PR's core goal).
2. **IPFS is the new, smaller bottleneck** — both free gateways (ipfs.io,
   gateway.pinata.cloud) 429 / connection-error under concurrency 8. The
   2-gateway failover IS rotating; the fix is lower concurrency (3-4) for the
   full re-drain, or more / dedicated gateways.
3. **`data:` URI class rejected (NEW, high-value)** — a large class (Sushiswap
   LP-position NFTs, ~⅓ of the batch) returns `data:application/json;base64,…`
   inline metadata, blocked by `validate_uri`'s scheme guard. Decoding `data:`
   is SSRF-safe (no network fetch) and would recover the class. **Separate
   follow-up** (URI-scheme support ≠ rotation) — not yet created.

- [ ] **Docs updated** — N/A (internal client; env/behaviour only, no
      architecture-shape change — no schema/endpoint/pipeline change).
- [ ] **API types regenerated** — N/A (no `crates/api/**` / `Cargo` DTO change).

## Notes — sieve results (2026-06-22, from the prod box)

**RPC pool (`SOROBAN_RPC_URLS`)** — keyless, all at ledger 63146999 (in sync):

- `https://mainnet.sorobanrpc.com` (SDF, 0.37s)
- `https://soroban-rpc.mainnet.stellar.gateway.fm/` (0.25s, fastest)
- `https://rpc.ankr.com/stellar_soroban` (1.64s)
- `https://stellar.api.onfinality.io/public` (1.47s)
- DROPPED: `gateway.tatum.io` → 429 on the first ping.

**IPFS pool** — path-style 200, box-reachable, byte-identical (sha256 match):

- `https://ipfs.io/ipfs/`
- `https://gateway.pinata.cloud/ipfs/`
- DROPPED: `cloudflare-ipfs.com` (000/dead), `dweb.link` (301+504),
  `4everland.io` / `w3s.link` / `nftstorage.link` (redirect), `flk-ipfs.xyz`
  (000).

4 RPCs → ~4× per-second budget + failover; should clear the ~1,450 backlog.
URLs live in env, **not** in code.
