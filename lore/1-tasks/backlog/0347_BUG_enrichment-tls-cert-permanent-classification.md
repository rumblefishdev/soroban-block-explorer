---
id: '0347'
title: 'BUG: classify TLS certificate-verification failures as permanent (sentinel) in enrichment, not transient'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0335', '0282', '0311']
tags:
  [
    enrichment,
    clickhouse,
    classification,
    tls,
    dlq,
    effort-small,
    priority-medium,
  ]
links: []
history:
  - date: '2026-07-03'
    status: backlog
    who: karolkow
    note: >
      Renumbered 0345 → 0347: id 0345 collided with pre-existing
      0345_PERF_entity-filtered-endpoints-skip-index-or-mv.md (spawned
      earlier in the 0338 perf-task batch). 0346 also taken, so next free
      id was 0347.
  - date: '2026-07-03'
    status: backlog
    who: karolkow
    note: >
      Spawned from the 2026-07-02/03 prod incident: the
      `production-enrichment-dlq-depth` alarm fired (20 messages in the
      enrichment DLQ). Root-caused to an issuer whose stellar.toml host
      serves a certificate that does not cover its domain — a PERMANENT
      TLS failure mis-classified as transient → 3× retry → DLQ. Direct
      sibling to 0335 (which fixed the DNS-NXDOMAIN case but deliberately
      left ALL TLS failures transient — see 0335 AC line "Connect-refused /
      TLS / timeout / 5xx / 429 stay transient").
  - date: '2026-07-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      **Re-measured on prod while closing 0335 — this got 560x worse, and it is
      live.** The alarm that spawned this task fired at **20** DLQ messages. Today
      `production-enrichment-dlq` holds **11,275**.
      Measured, not inferred: DLQ retention is **14 days** and sampled messages
      were sent **2026-07-16**, so the entire population post-dates the 0335 deploy
      (07-02) — this is ongoing intake, not a stale backlog. A 5-message sample came
      back **5/5 identical**: the same asset (wBTC, issuer `GDVIQFRC…ATMI`,
      home_domain `atmindividual.org`) re-enqueued over and over. So the flood is a
      *hot loop on a few broken issuers*, not broad noise — the producer re-emits
      each un-enriched asset, the fetch fails identically, 3x retry, DLQ, repeat.
      Confirmed the failure is this task's class and NOT 0335's: `atmindividual.org`
      **resolves fine** (69.57.162.184, NOERROR, live NS + MX) and fails on
      `no alternative certificate subject name matches target host name` — a cert
      name-mismatch, deterministically permanent, exactly the case in the Summary.
      0335's DNS branch is working correctly; this class is simply left uncovered by
      design.
      Practical note for whoever picks this up: the main queue is drained (0) and
      the worker is live (ESM Enabled, batch 10), so the DLQ is the only symptom
      surface — and a `--retry-sentinels`-style re-drain will NOT help until the
      classifier changes, because every retry reproduces the same cert error.
---

# BUG: TLS certificate-verification failure → permanent (sentinel), not transient

## Summary

A stellar.toml (SEP-1) / `token_uri` fetch that fails TLS **certificate
verification** (name mismatch, expired, untrusted root, self-signed) is
currently classified **transient** by both `is_transient` functions. A
cert-verification failure is deterministically **permanent** — it repeats
identically on every retry until the _issuer_ fixes their server — so the
worker burns `maxReceiveCount = 3` retries and then drops the message into
the enrichment DLQ, tripping the `production-enrichment-dlq-depth` alarm.
Make cert-verification failures permanent (write the `''` sentinel + ack),
while keeping genuinely-transient TLS transport failures (handshake EOF,
connection reset) on the retry path.

## Context

### The incident (2026-07-02/03)

- Alarm `production-enrichment-dlq-depth` fired: 20 messages in
  `production-enrichment-dlq`.
- The DLQ-driving log line (worker `ERROR`, "reporting partial batch
  failure"):
  `transient enrichment fetch error: HTTP error fetching stellar.toml from
atmindividual.org: error sending request for url
(https://atmindividual.org/.well-known/stellar.toml)`
- Live probe of the domain (2026-07-03):
  - **Resolves** → `69.57.162.184` (Namecheap shared hosting). So NOT a
    DNS failure — 0335's `is_dns_failure` correctly returns false.
  - **HTTPS fails on the certificate**:
    `SSL: no alternative certificate subject name matches target host name
'atmindividual.org'` — a parked/shared-host default cert that does not
    cover the domain.
- The other log line that day (`token_uri_permanent`, `Error(WasmVm,
MissingValue)`) is **unrelated** — that is a contract lacking a
  `token_uri` entrypoint, already correctly permanent post-0335. It is
  WARN-level noise, not a DLQ contributor.

### Why the code mis-classifies it

There are TWO classifiers (same as 0335 documents):

- **SEP-1** [`sep1_assets::is_transient(&Sep1Error)`](../../../crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs)
  — the `Sep1Error::Http { source }` arm with no HTTP status does
  `None => !is_dns_failure(source)`. A TLS cert error has no HTTP status and
  is not a DNS failure → returns **true** (transient).
- **NFT** [`nft_token_uri::errors::is_transient(&NftTokenUriError)`](../../../crates/enrichment-shared/src/nft_token_uri/errors.rs)
  — the `Http { source }` arm returns true for `source.is_connect()`; a
  rustls cert-verification failure surfaces as a connect-phase error →
  **true** (transient).

0335 deliberately lumped all TLS into "transient" (its premise: an ambiguous
connect failure might be a temporarily-down site; cost asymmetry favours
retry). That default is correct for _transport_ TLS errors, but a
cert-**verification** failure is unambiguously permanent, exactly like
NXDOMAIN.

### TLS backend fact (drives the fix)

The workspace pins `reqwest = { default-features = false, features =
["rustls-tls", "json"] }` (`Cargo.toml`). **rustls** renders every
certificate-validation failure with the umbrella prefix
`invalid peer certificate: <reason>` (e.g. `NotValidForName`, `Expired`,
`UnknownIssuer`). Transport-level TLS errors (handshake EOF, connection
reset, protocol error) do **not** carry that prefix. So a single marker —
`"invalid peer certificate"` — cleanly separates permanent
(cert-validation) from transient (transport), with no risk of catching the
recoverable cases.

## Implementation Plan

Mirror 0335's shape exactly (a `pub(crate)` helper in
`nft_token_uri::errors`, imported by `sep1_assets`, plus a unit-testable
`*_marker` string matcher).

### Step 1 — detect a cert-verification failure

Add next to `is_dns_failure` / `is_dns_marker`:

```rust
/// True when a reqwest error chain indicates TLS *certificate verification*
/// failed (name mismatch / expired / untrusted root / self-signed).
/// PERMANENT: the issuer's cert won't fix itself on retry — sentinel, not
/// 3×-retry → DLQ. rustls renders ALL cert-validation failures as
/// "invalid peer certificate: <reason>"; transport TLS errors (handshake
/// eof / reset) do not, so they stay transient.
pub(crate) fn is_cert_failure(err: &reqwest::Error) -> bool { /* walk source chain */ }

/// ponytail: string-match on rustls cert-error text — the workspace pins
/// rustls-tls; upgrade to a typed rustls error downcast if the backend or
/// rustls phrasing changes.
fn is_cert_marker(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("invalid peer certificate")
}
```

### Step 2 — branch it permanent in both `is_transient` fns

- SEP-1: in the `Sep1Error::Http { source }` `None` arm, return `false`
  when `is_cert_failure(source)` (alongside the existing `is_dns_failure`
  guard).
- NFT: in the `NftTokenUriError::Http { source }` arm, add
  `!is_cert_failure(source) && …` (alongside the existing `!is_dns_failure`).
- Leave `is_endpoint_fault` UNCHANGED — a cert-broken host in the RPC/IPFS
  pool should still fail over to a different endpoint (same reasoning 0335
  used for DNS).

### Step 3 — tests

- `is_cert_marker` true for `"invalid peer certificate: NotValidForName"`,
  `"…: Expired"`, `"…: UnknownIssuer"`; false for transport phrasings
  (`"unexpected eof"`, `"connection reset"`, `"handshake"`, `"tls handshake
eof"` — the latter is already guarded transient by
  `dns_marker_ignores_transient_phrasings`; add the cert analog).
- Both `is_transient` fns: a cert-failure `Http` error → `false`.
- **Empirical guard (`#[ignore]` net test, mirrors
  `is_dns_failure_matches_real_reqwest_nxdomain`)**: hit a known
  cert-mismatch host and assert `is_cert_failure` fires against reqwest's
  REAL rustls output. Use `https://wrong.host.badssl.com/` (name mismatch)
  and/or `https://expired.badssl.com/`. This locks the marker to the actual
  rustls-via-reqwest text on the prod platform (Lambda AL2), the same way
  0335 locked the DNS text.

## Acceptance Criteria

- [ ] `is_cert_failure` + `is_cert_marker` helper; TLS cert-verification
      failures classified permanent (sentinel + ack).
- [ ] Transport-level TLS failures (handshake EOF / reset) stay transient
      (unchanged) — guarded by a unit test.
- [ ] Applies to BOTH classifiers — SEP-1 (`sep1_assets::is_transient`) and
      NFT (`nft_token_uri::errors::is_transient`). `is_endpoint_fault`
      intentionally unchanged.
- [ ] `#[ignore]` net test verifies the marker vs real reqwest+rustls output
      (badssl.com cert-mismatch host).
- [ ] After deploy: cert-broken issuer domains no longer reach the DLQ; the
      existing 20-message backlog can be drained (redrive → they sentinel and
      ack, or purge). **Deferred — needs build + deploy.**
- [ ] **Docs updated** — N/A (internal classifier behaviour; no schema /
      endpoint / pipeline-shape change).
- [ ] **API types regenerated** — N/A (no `crates/api/**` / Cargo DTO change).

## Alternatives considered

- **Pre-flight cert check** (validate the cert before the fetch) — redundant;
  the fetch already surfaces the failure with a classifiable error.
- **Age-based escalation** (sentinel after N days) — heavier (needs a
  cross-attempt ledger); 0335 already rejected this for the DNS case. The
  cert-string approach is unified with the DNS approach.

## Notes

- Same family as 0335 (DNS-NXDOMAIN) and 0282 (NFT media-url host-unreachable
  classification). This one closes the _cert-verification_ leg 0335 explicitly
  left transient.
- Sentinels remain RMT-upgradeable: a later `enrich --retry-sentinels` re-runs
  them, so an issuer that fixes its cert is recoverable — no permanent data
  loss.
- Impact is low-severity: enrichment is fail-soft (affected tokens render
  without name/icon; no user-facing error). This task removes DLQ-alarm noise,
  not a user-facing outage.
- Ops follow-up (separate, needs prod access): confirm the 20 DLQ messages are
  the cert-broken issuers, then drain the DLQ once the fix ships.
