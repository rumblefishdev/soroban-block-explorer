---
id: '0333'
title: 'FEATURE: classify DNS-resolution failures as permanent (sentinel) in enrichment, not transient'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0311', '0306', '0231']
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

- Classifier: [`is_transient`](../../../crates/enrichment-shared/src/nft_token_uri/errors.rs)
  — single decision point, used by **both** the NFT path
  ([nft_token_uri.rs](../../../crates/enrichment-shared/src/enrich_and_persist/nft_token_uri.rs))
  and the SEP-1 path
  ([sep1_assets.rs:110](../../../crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs)).
  `is_transient == false` → enrich fn writes the `''` sentinel + returns
  `Ok` (acked, candidate-query skips it next pass).
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

- [ ] `is_dns_failure` helper; DNS-resolution failures classified permanent.
- [ ] Connect-refused / TLS / timeout / 5xx / 429 stay transient (unchanged).
- [ ] Unit tests for both classes.
- [ ] Applies to both NFT + SEP-1 paths (shared `is_transient`).
- [ ] After deploy: a `enrich sep1-assets` pass moves the dead-domain tail
      from `transient` to `sentinel` (drain converges; candidate count drops).
- [ ] Lambda steady-state: dead-domain assets no longer reach the DLQ.

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
