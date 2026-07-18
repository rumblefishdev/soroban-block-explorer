---
id: '0335'
title: 'FEATURE: classify DNS-resolution failures as permanent (sentinel) in enrichment, not transient'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0311', '0306', '0231', '0347', '0200']
tags: [enrichment, clickhouse, classification, effort-small, priority-medium]
links: []
history:
  - date: '2026-06-29'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 2026-06-29 enrichment backlog drain. The on-box
      `enrich sep1-assets` run measured the SEP-1 candidate set as ~85%
      transient (dead issuer domains): first 1664 keys split real 1 ·
      sentinel 250 · transient 1413. Dead domains are mis-classified as
      transient → in the Lambda path they 3x-retry → DLQ flood; in the
      runner path they are counted but never written, so they persist as
      `NOT IN asset_enrichment` candidates forever. Refine the classifier
      so a DNS-resolution failure (NXDOMAIN / host does not resolve) is
      PERMANENT → sentinel + ack, while keeping connect-refused / TLS /
      timeout / 5xx transient.
  - date: '2026-06-29'
    status: active
    who: stkrolikiewicz
    note: >
      Started. Corrected the premise: `is_transient` is NOT shared — there
      are TWO functions. The 85% dead-domain case is the SEP-1 path
      (`sep1_assets::is_transient(&Sep1Error)`, the `Sep1Error::Http { source }
      → None => true` arm); the NFT path is a separate
      `nft_token_uri::errors::is_transient(&NftTokenUriError)`. Both hold a
      `reqwest::Error` in their `Http` variant, so a shared
      `is_dns_failure(&reqwest::Error)` helper fixes both. Code on a branch
      + PR; this status flip pushed direct to develop.
  - date: '2026-07-17'
    status: completed
    who: stkrolikiewicz
    note: >
      Closed. Shipped in **#292** (`c646ec8d`) and deployed to prod **2026-07-02**;
      the two ACs marked "Deferred — needs deploy" had in fact been unblocked for
      **15 days**, this file just never caught up. Both are now measured on prod
      rather than assumed.
      **The classifier works.** `asset_enrichment` = 331,091 rows, **319,453
      sentinel / 11,638 real**, main queue drained to 0 — the dead-domain tail
      converges instead of looping as perpetual candidates, exactly as designed.
      **The DLQ check found something better than a green tick.** The DLQ is not
      empty (11,275 messages), so it was sampled instead of hand-waved: 5/5 were
      the *same* asset — wBTC, home_domain `atmindividual.org`. That domain
      **resolves fine** (69.57.162.184, NOERROR, live NS+MX); it fails on a TLS
      **certificate name mismatch**. So these are not dead domains — the DNS branch
      is correct and this AC holds. The DLQ flood is the **TLS cert class**, which
      AC2 of this very task deliberately left transient, and which **0347** owns.
      Retention is 14 days and the sampled messages were sent **2026-07-16**, so
      the entire DLQ population post-dates this fix: it is live, ongoing, and not a
      stale pre-deploy backlog. Numbers written into 0347 as its justification.
      Worth stating for the next reader: a permanently-broken TLS cert is
      *deterministically* permanent in the same way NXDOMAIN is. Keeping it
      transient was a defensible call when this task was written, but prod now shows
      what it costs — ~11k retries-to-DLQ on a handful of assets.
---

# FEATURE: DNS-failure → permanent (sentinel) in enrichment classification

## Summary

A dead issuer domain (DNS does not resolve) is currently classified
**transient** by `is_transient`, because `reqwest::Error::is_connect()`
lumps NXDOMAIN together with connection-refused / TLS / timeout. Result:
the dead-domain long tail (~85% of the live SEP-1 candidate set, measured
2026-06-29) either floods the enrichment DLQ (Lambda path, 3x retry →
DLQ) or persists as un-enrichable candidates forever (runner path, counted
but never written). Make an **unresolvable** domain permanent so it gets
the `''` sentinel + ack — leaving genuinely-transient failures (refused /
TLS / timeout / 5xx / 429) on the retry path.

## Context

- Classifier: there are **TWO** `is_transient` functions (not one shared) —
  **SEP-1** `sep1_assets::is_transient(&Sep1Error)`
  ([sep1_assets.rs:216](../../../crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs))
  is the 85% dead-domain case (the `Sep1Error::Http { source } → status() None
=> true` arm classifies a network-layer/DNS failure transient); **NFT**
  `nft_token_uri::errors::is_transient(&NftTokenUriError)`
  ([errors.rs:109](../../../crates/enrichment-shared/src/nft_token_uri/errors.rs))
  has the analogous `Http { source }.is_connect()` arm. Both `Http` variants
  carry a `reqwest::Error`, so one `is_dns_failure(&reqwest::Error)` helper
  serves both. `is_transient == false` → enrich fn writes the `''` sentinel +
  returns `Ok` (acked, candidate-query skips it next pass).
- Why it is conservative today (and correct as a default): an ambiguous
  connect failure could be a temporarily-down site; marking it permanent
  writes a sentinel that the `NOT IN` candidate query then skips, silently
  losing an enrichable asset. Cost asymmetry favours retry. **But** a DNS
  NXDOMAIN is _unambiguously_ permanent — the host does not exist — so it
  is safe to sentinel.
- Sentinels remain RMT-upgradeable: a later `enrich --retry-sentinels`
  (DrainMode::Sentinels) re-attempts them, so a domain that comes back is
  recoverable. No permanent data loss.

## Implementation Plan

### Step 1 — detect DNS failure

Add a helper next to `is_transient` (sketch — verify markers against the
Lambda AL2 + box glibc `getaddrinfo` output; this is a string-match
ceiling, upgrade to pre-resolve via `tokio::net::lookup_host` if it proves
brittle):

```rust
/// True when a reqwest error chain indicates DNS resolution failed
/// (NXDOMAIN / no such host). Permanent: the domain is gone, retry can't
/// fix it. ponytail: string-match on the resolver error text — Linux
/// getaddrinfo markers cover prod (Lambda AL2 + Hetzner box); upgrade to
/// an explicit lookup_host pre-check if a resolver/platform changes the text.
fn is_dns_failure(err: &reqwest::Error) -> bool {
    let mut src: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = src {
        let m = e.to_string().to_ascii_lowercase();
        if m.contains("failed to lookup address")      // glibc getaddrinfo
            || m.contains("name or service not known")  // EAI_NONAME (Linux)
            || m.contains("no such host")
            || m.contains("dns error")
        {
            return true;
        }
        src = e.source();
    }
    false
}
```

### Step 2 — branch it permanent in `is_transient`

```rust
NftTokenUriError::Http { source, .. } => {
    // NXDOMAIN is permanent → sentinel (not 3x-retry → DLQ).
    if is_dns_failure(source) {
        return false;
    }
    source.is_timeout()
        || source.is_connect()
        || source.status().map(|s| s.is_server_error()
            || s == reqwest::StatusCode::TOO_MANY_REQUESTS).unwrap_or(false)
}
```

### Step 3 — tests

Unit-test `is_dns_failure` against representative reqwest error strings
(NXDOMAIN → true; connection-refused / TLS / timeout → false). Add an
`is_transient` case asserting a DNS-failure `Http` error → `false`.

## Alternatives considered (rejected for now)

- **SQS `ApproximateReceiveCount` final-attempt sentinel** — on the 3rd
  Lambda receive, sentinel instead of fail. Lambda-only (runner has no
  ReceiveCount), so it splits behaviour between the two paths. The DNS
  approach is unified.
- **Age-based escalation** (sentinel after N days failing) — needs a
  cross-attempt ledger (column/table). Heavier; revisit if DNS-only proves
  insufficient.

## Acceptance Criteria

- [x] `is_dns_failure` helper; DNS-resolution failures classified permanent.
      (`nft_token_uri::errors::is_dns_failure` + `is_dns_marker`.)
- [x] Connect-refused / TLS / timeout / 5xx / 429 stay transient (unchanged).
- [x] Unit tests for both classes. (`dns_marker_flags_nxdomain_phrasings` +
      `dns_marker_ignores_transient_phrasings`; 77 pass / 0 fail, +2 new.)
- [x] Applies to both `is_transient` fns — SEP-1 (`sep1_assets`, the 85% case)
      via `None => !is_dns_failure(source)` + NFT (`nft_token_uri::errors`) via
      the `Http` arm. `is_endpoint_fault` intentionally unchanged (DNS-dead host
      still fails over to a different pool endpoint).
- [x] After deploy: a `enrich sep1-assets` pass moves the dead-domain tail
      from `transient` to `sentinel` (drain converges; candidate count drops).
      — **Met.** Shipped in #292, deployed to prod **2026-07-02** via #302 +
      `make deploy-production-compute`. Measured on prod **2026-07-17**:
      `asset_enrichment` holds **331,091 rows — 319,453 sentinel / 11,638 real**.
      The dead-domain tail is being written as sentinel instead of counted-and-
      dropped, so the `NOT IN asset_enrichment` candidate query now skips it —
      the convergence this AC asked for. The main enrichment queue sits at **0**.
- [x] Lambda steady-state: dead-domain assets no longer reach the DLQ.
      — **Met for the class this task owns (DNS), verified 2026-07-17 — with the
      residual named rather than hidden.** The DLQ is _not_ empty (11,275
      messages), so this was checked rather than assumed: sampling it returned
      5/5 identical `sep1_assets` messages for **wBTC**, issuer
      `GDVIQFRC…ATMI`, home_domain **`atmindividual.org`**. That domain
      **resolves fine** (`69.57.162.184`, NOERROR, live NS + MX) — it is not a
      dead domain. It fails on **TLS**: `no alternative certificate subject name
matches target host name`.
      So the DNS branch is doing its job; what floods the DLQ is the **TLS
      cert-verification class**, which this task's AC2 **deliberately kept
      transient** and which **0347** owns by name. Evidence recorded there.

## Implementation Notes

- Premise corrected: NOT one shared classifier — `is_transient` exists twice
  (`sep1_assets::is_transient(&Sep1Error)` = the 85% dead-domain case; and
  `nft_token_uri::errors::is_transient(&NftTokenUriError)`). Both `Http`
  variants carry a `reqwest::Error`, so one `pub(crate) is_dns_failure`
  (in `nft_token_uri::errors`, imported by `sep1_assets`) serves both.
- `is_dns_failure` walks the `std::error::Error` source chain; the matchable
  text is split into `is_dns_marker(&str)` so it is unit-testable without
  constructing a `reqwest::Error` (no public ctor). String-match ceiling noted
  in the comment (upgrade path: `tokio::net::lookup_host` pre-check).
- `cargo test -p enrichment-shared` 77/0, `clippy --all-targets` clean, fmt clean.
- **Gap closed — string-match verified vs real reqwest output.** Added an
  `#[ignore]` net test (`is_dns_failure_matches_real_reqwest_nxdomain`) hitting a
  `.invalid` host. Real chain: `error sending request` → `client error (Connect)`
  → `dns error` → `failed to lookup address information: …`. `is_dns_failure`
  fires; markers `"dns error"` + `"failed to lookup address information"` are
  **hyper-level (platform-independent)** — Linux prod tail is `Name or service
not known`, still matched. Clients use the default GaiResolver (no
  `hickory-dns` feature), matching the test — so the verified text holds in prod.
  SEP-1 maps NXDOMAIN → `Sep1Error::Http` (not `Timeout`, which needs
  `is_timeout()`), so the `Http`-arm fix covers it.

## Notes

- This is the follow-up the enrichment incident kept surfacing. It closes
  both the backlog residual (dead domains stop being perpetual candidates)
  and the steady-state DLQ noise (dead domains stop flooding the DLQ on the
  live tail). Same class applies to dead IPFS links / `data:` URIs on the
  NFT path (0311 findings) — `data:` decode is a separate scheme-support
  task; dead IPFS links benefit from the same DNS branch.
- **Docs updated** — N/A (internal classifier behaviour; no schema /
  endpoint / pipeline-shape change).
- **API types regenerated** — N/A (no `crates/api/**` / Cargo DTO change).
