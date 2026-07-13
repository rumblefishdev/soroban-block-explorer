---
id: '0200'
title: 'SEP-1 fetcher: follow same-eTLD+1 redirects (publicsuffix)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0188']
tags: [priority-low, effort-small, layer-backend, enrichment, sep1, security]
milestone: 2
links:
  - https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0001.md
  - https://crates.io/crates/publicsuffix
history:
  - date: '2026-05-07'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0188 follow-up. Smoke-testing PR #157 against local DB
      revealed that issuers whose `home_domain` is the apex (e.g. `circle.com`)
      lose all SEP-1 enrichment because the canonical TOML lives under `www.`
      and the fetcher refuses to follow 30x (`Policy::limited(0)`, by design,
      per `client.rs` docstring). Real-world impact: most corporate issuers
      (Circle USDC/EURC etc.) → 0% enrichment until they fix on-chain
      `home_domain` flag, which requires `set_options` op + multisig +
      coordination → unlikely to happen.
  - date: '2026-07-02'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted — confirmed as the root cause of the flagship-asset enrichment
      gap (Circle USDC/EURC serve null `name`/`icon_url`). Live check 2026-07-01:
      `circle.com/.well-known/stellar.toml` → 301 → `www.circle.com`, and the
      SEP-1 fetcher's `Policy::limited(0)` drops the redirect → sentinel. Both
      the read-time (`api::runtime_enrichment::sep1`) and enrichment
      (`enrichment-shared::sep1`) fetchers are affected. Implementing the
      same-eTLD+1 redirect follow (publicsuffix + `validate_host` re-applied per
      Location to keep the SSRF block intact).
  - date: '2026-07-13'
    status: completed
    who: stkrolikiewicz
    note: >
      Shipped in PR #302 (merged develop). `psl`-based same-eTLD+1 redirect
      policy on both sep1 fetchers (validate_host re-applied per Location,
      MAX_REDIRECTS cap); enrichment worker enabled (concurrency 0->1) and
      `--retry-sentinels` recovered ~684 real enrichments. circle.com/USDC/EURC
      confirmed EXTERNAL (serve no stellar.toml), not a fetcher bug. Verified via
      unit tests + prod backfill.
---

# SEP-1 fetcher: follow same-eTLD+1 redirects (publicsuffix)

## Summary

Allow the SEP-1 fetcher to follow a bounded number of HTTP redirects when the destination registers under the same eTLD+1 as the origin host (so `circle.com` → `www.circle.com` succeeds, `circle.com` → `evil.com` does not). Reapply `validate_host` on every Location to keep IP-literal SSRF blocks intact. Re-enables enrichment for the apex↔www-redirect class of issuers without re-opening the SSRF gate that `Policy::limited(0)` closes today.

## Context

Task 0188 / PR #157 ships `runtime_enrichment::sep1` with **redirects disabled** (`reqwest::redirect::Policy::limited(0)`). The `client.rs` module docstring documents the trade-off:

> "legitimate issuers behind apex↔www-style redirects simply get null `description` / `home_page` until their `home_domain` flag matches the canonical TOML host directly."

Local smoke testing (5 of 7 assets-with-home_domain test, see 0188 follow-up notes) confirmed:

- `whitehats.cc` → no redirect → full enrichment ✓
- `circle.com` → 30x → entire fetch errors out, both fields null ✗
- `jfkxrpl.com`, `xlmrefund.com` → no redirect (when reachable) → full enrichment ✓

In the wild the `circle.com` shape is the **majority** case for any non-crypto-native issuer (apex domain hosts marketing redirect, content sits under `www.` or a CDN). Issuers fixing their on-chain `home_domain` is theoretically possible but practically rare — costs an XLM `set_options` op, often needs multisig, and many issuers no longer have an active dev team.

Per-explorer comparison (anecdotal, not yet benchmarked): `stellar.expert` and `stellarbeat` both follow redirects for SEP-1, presumably with some host-scope guard.

The follow-up needs to thread the needle: re-enable the realistic redirect case **without** weakening the SSRF guarantees of validate_host. Same-eTLD+1 enforcement via the public suffix list is the standard primitive.

## Implementation Plan

### Step 1: Workspace dep

Add `publicsuffix = "2"` (or `psl = "2"` — pick the one with a vendored PSL list and no I/O) to `[workspace.dependencies]`. Add to `crates/api/Cargo.toml`. PSL data is shipped vendored; refresh on `cargo update` cadence.

### Step 2: Custom redirect policy on `Sep1Fetcher`

Replace `Policy::limited(0)` with `Policy::custom(|attempt| ...)`:

```rust
.redirect(reqwest::redirect::Policy::custom(|attempt| {
    if attempt.previous().len() >= MAX_REDIRECTS {
        return attempt.error("too many redirects");
    }
    let prev_host = attempt.previous().last()
        .and_then(|u| u.host_str())
        .unwrap_or("");
    let next_host = attempt.url().host_str().unwrap_or("");
    // Reapply the same RFC 1035 + IP literal guard validate_host runs on
    // the initial host, on every redirect Location.
    if validate_host(next_host).is_err() {
        return attempt.error("redirect to unsafe host");
    }
    // Same-eTLD+1 check: registrable domain must match.
    if registrable_domain(prev_host) != registrable_domain(next_host) {
        return attempt.error("redirect leaves origin eTLD+1");
    }
    attempt.follow()
}))
```

Constants:

- `MAX_REDIRECTS: usize = 2` — enough for apex → www → CDN; tight enough that fetch budget doesn't blow.

### Step 3: `registrable_domain` helper

Wraps `publicsuffix::List::registrable_domain` (or `psl::domain_str`). Returns `Option<&str>`. None → reject (private / weird hosts that the PSL can't classify).

### Step 4: Tests

Add to `client.rs::tests` (still no fixture HTTPS server — pure unit / synthetic URL):

- `redirect_policy_allows_apex_to_www_same_etld1` — feed a synthetic `Attempt` for `circle.com` → `www.circle.com`, expect follow.
- `redirect_policy_blocks_cross_etld1` — `circle.com` → `evil.com`, expect error.
- `redirect_policy_blocks_to_ip_literal` — `issuer.com` → `127.0.0.1` (would happen if DNS lies); expect error via `validate_host`.
- `redirect_policy_blocks_after_max_hops` — chain of `MAX_REDIRECTS + 1` same-eTLD+1 hops, expect error.
- `registrable_domain_handles_co_uk_style` — `api.example.co.uk` → `example.co.uk` (PSL multi-level TLD).

Constructing `reqwest::redirect::Attempt` directly is non-trivial (private constructor); may require a small wrapper that takes `(prev_url, next_url)` and runs the same predicate logic. Acceptable.

### Step 5: End-to-end verification

After merge, smoke-test against a local DB seeded with `accounts.home_domain = 'circle.com'` for an asset that has a matching `[[CURRENCIES]]` entry in `circle.com`'s TOML (e.g. native USDC issuer `GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`). Expect: HTTP 200, `description` populated from `CURRENCIES[].desc`, `home_page = "https://www.circle.com"`.

### Step 6: Docs

Update `client.rs` module docstring: drop the "issuers behind apex↔www-style redirects simply get null fields" caveat; replace with the new same-eTLD+1 follow rule + MAX_REDIRECTS budget.

Update `docs/architecture/backend/backend-overview.md` §4.1 sep1 sub-bullet to mention the redirect policy.

## Acceptance Criteria

- [x] `publicsuffix` (or `psl`) workspace dep added with vendored PSL list.
- [x] `Sep1Fetcher::new()` uses `Policy::custom` enforcing: same-eTLD+1, validate_host on every Location, max 2 redirects.
- [x] At least 5 unit tests covering: same-eTLD+1 follow, cross-eTLD+1 block, IP-literal block on redirect, max-hops block, multi-level TLD (`co.uk`).
- [x] Manual smoke test — mechanism verified; `circle.com` itself turned out external (serves no `stellar.toml`, apex 301→www→404), so it was never a valid fixture. Redirect-follow validated in prod: `--retry-sentinels` recovered ~684 real enrichments.
- [x] Module docstring + `backend-overview.md` updated to reflect new policy.
- [x] `cargo check -p api`, `cargo clippy -p api -- -D warnings`, `cargo test -p api` clean.
- [x] **Docs updated** — `crates/api/src/runtime_enrichment/sep1/client.rs` module docstring + `docs/architecture/backend/backend-overview.md` §4.1. Mark `N/A` for other docs files.
- [x] **API types regenerated** — `N/A` — pure internal change, response shape unchanged.

## Out of Scope

- DNS-resolved private-IP SSRF block (resolve domain → check against RFC 1918 / 6598 / link-local). Still 0188 §"Out of Scope"; this task only handles literal IPs in redirect Locations, same as the initial `validate_host` does.
- Caching of redirect targets / canonical-host rewriting at index time. Runtime-only fix.
- Issuer-side `home_domain` migration tooling. Out of scope for backend.
- Per-issuer override list ("for circle.com, always use www.circle.com"). Avoid hard-coded mappings; let the public suffix list do the structural work.

## Notes

**Severity / priority rationale.** Tagged `priority-low` because:

- 0188 ships fail-soft (null fields, never 5xx) — this is enhancement, not bug fix.
- No frontend has shipped against `description` / `home_page` yet (PR #157 is the first to populate them).
- Real-world impact will be quantifiable post-deploy via warn-log volume / `home_domain` distribution analysis.

If post-launch metrics show > 50% of fetches failing on 30x for issuers with non-empty `home_domain`, bump to `priority-medium`.

**Why not heuristic www. prefix retry?** Considered as a quick win (zero deps, ~10 LOC) but rejected:

- Doesn't cover `m.example.com` ↔ `example.com` or `toml.example.com` ↔ `example.com`.
- Doubles request budget on apex fail (2 × 2 s = 4 s).
- Hard-coded heuristic ages badly.

The publicsuffix-based approach is structurally correct and a one-time integration cost.
