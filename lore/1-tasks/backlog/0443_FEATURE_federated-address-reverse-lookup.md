---
id: '0443'
title: 'FEATURE: resolve SEP-2 federated addresses (name*domain) for accounts that set a home domain'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0188']
tags:
  [backend, enrichment, frontend, accounts, sep2, priority-low, effort-medium]
links:
  - 'SEP-2 (Federation Protocol): https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0002.md'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/363'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: "if there's a
      federated address tied to this source account, it would be great to show
      it". Not implemented anywhere today — no prior task covers SEP-2.
---

# FEATURE: SEP-2 federated address reverse lookup

## Summary

Where an account has a federated address (`name*domain.com`, SEP-2), show it
next to the raw `G…` StrKey on the account detail page and in the transaction
source-account row. Requires a reverse (`type=id`) federation lookup, cached as
an enrichment field — never resolved on the read path.

## Context

Nothing in the codebase performs any federation lookup. Two of the three
required pieces already exist:

- `accounts.home_domain` is indexed and stored
  (`crates/enrichment-worker/src/main.rs:377`); it is mutable via `SET_OPTIONS`,
  so the enrichment path already treats it as a moving value
  (`crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs:79-99`).
- A `stellar.toml` fetcher exists from the SEP-1 asset-details work
  (task 0188, `crates/enrichment-shared/src/sep1/`).

Missing: reading `FEDERATION_SERVER` out of the fetched toml and issuing the
reverse query.

## Implementation sketch

1. Extend the SEP-1 toml parse to capture `FEDERATION_SERVER`.
2. New enrichment step: for accounts with a non-empty `home_domain`, call
   `GET <federation_server>?q=<G-strkey>&type=id`, store `stellar_address`.
3. Surface the field on the account detail DTO + transaction source-account row.
4. Frontend: render `name*domain.com` above the truncated StrKey; StrKey stays
   the copyable canonical value.

## Constraints / risks

- **Off the read path.** The lookup hits a third-party HTTP server. It must run
  in enrichment with a cached result + TTL, never during a page request.
- **Low coverage.** Only accounts that set `home_domain` can ever resolve;
  in practice mostly exchanges and custodial wallets.
- **Not authoritative.** A federation server is operated by the domain owner and
  can return anything. Display it as a claim of that domain, never as a verified
  identity, and never let it replace the StrKey as the copyable value.
- Needs a negative cache — most lookups will 404 and must not be retried hot.
- **`FEDERATION_SERVER` is a URL an untrusted third party chose.** We fetch it
  because an account set a home domain, so the destination is attacker-supplied
  in the ordinary case, not the exotic one. Whoever writes this must treat the
  fetch as SSRF-shaped from the start — retrofitting the guards after the first
  fetch works is how this class of bug ships. The concrete requirements are in
  the acceptance criteria rather than here, so they cannot be read as advice.

## Acceptance criteria

- [ ] `FEDERATION_SERVER` parsed from `stellar.toml`
- [ ] **The URL is validated before it is fetched:** HTTPS only, and the
      resolved address rejected when it is loopback, link-local, RFC1918 or a
      cloud metadata endpoint. Check after DNS resolution, not on the hostname —
      a hostname resolving to `169.254.169.254` passes any string check.
- [ ] **Every redirect target re-validated the same way**, and the redirect
      chain bounded — one allowed hop is enough to reach the metadata service.
- [ ] **Connect and read timeouts, a response-size cap, and bounded retries**,
      so a hostile or hung server cannot pin an enrichment worker
- [ ] Reverse (`type=id`) lookup runs in enrichment, with TTL + negative cache
- [ ] Resolved address exposed on the account DTO and tx source-account row
- [ ] Frontend renders it as secondary to the StrKey, not as a replacement
- [ ] Absent / failed lookup degrades silently to StrKey-only
- [ ] **Docs updated** — `docs/architecture/**` enrichment pipeline gains one
      step; update in the same PR per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`

## Notes

Worth scoping the coverage before building: count accounts with a non-empty
`home_domain`, then sample how many of those domains actually serve a
`FEDERATION_SERVER`. If the intersection is negligible, close this instead.
