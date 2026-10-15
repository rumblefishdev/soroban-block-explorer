---
id: '0643'
title: 'SECURITY: one outbound-host guard — refuse non-global addresses for every third-party fetch'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0188', '0200', '0455', '0639']
tags: ['security', 'ssrf', 'enrichment', 'effort-small']
links:
  - https://github.com/stellar/stellar-protocol/pull/2038
history:
  - date: 2026-10-15
    status: active
    who: karolkow
    note: 'Task created'
---

# SECURITY: one outbound-host guard — refuse non-global addresses for every third-party fetch

## Summary

The API Lambda and the enrichment worker fetch URLs that come from chain data
someone else wrote: an issuer's `home_domain` (SEP-1 `stellar.toml`) and an
NFT contract's `token_uri`. Today these clients refuse IP literals and
non-https URLs, but a DNS name that resolves to `127.0.0.1`,
`169.254.169.254` (instance metadata) or a private range is fetched. One
shared guard resolves the host, refuses every address that is not globally
reachable, and hands the checked addresses to the connection, so nothing
re-resolves between the check and the connect.

## Context

- The contract verification SEP draft (stellar-protocol PR 2038, security
  section) names a server-side reader a confused deputy and asks it to resolve
  and reject loopback, link-local and private-range destinations.
- `crates/enrichment-shared/src/sep1/client.rs` says so itself in its header:
  "DNS-resolved private addresses are NOT blocked at this layer".
- No check of resolved addresses exists in `crates/enrichment-shared/src` or
  `crates/api/src/runtime_enrichment` (grep for `is_private`, `is_loopback`,
  `is_global`, `dns::Resolve`: 0 hits, 2026-10-15).
- Clients that fetch a third-party URL (all `reqwest::Client::builder()` sites
  on `develop`, 2026-10-15): SEP-1 fetcher, NFT token URI fetcher. The others
  (RPC pool, history archives, Turnstile, ClickHouse mTLS, load tests) call
  hosts set in configuration.
- Task 0639's verified-source client (PR 661, not yet on `develop`) downloads
  an archive from the verifier's host named in a third-party answer — it takes
  the same guard once both are on `develop`.

## Implementation Plan

1. `enrichment-shared::outbound_guard`: a `reqwest::dns::Resolve` that looks
   the name up, refuses the host when any address is in the IANA
   special-purpose registries (crate `iprfc`, `RFC6890` aggregate — generated
   from the IANA IPv4/IPv6 Special-Purpose Address Space registries), after
   unwrapping IPv4-mapped IPv6 (`to_canonical`), and returns the checked
   addresses to reqwest.
2. SEP-1 client: `.dns_resolver(...)`.
3. NFT client: metadata fetches on their own guarded client; Soroban RPC
   calls keep the unguarded client (configured hosts, local dev on
   `localhost`).
4. Refusal is a distinct error variant, logged with its cause.

## Acceptance Criteria

- [ ] A host resolving to loopback / private / link-local / CGNAT / other
      IANA special-purpose addresses (v4 and v6, IPv4-mapped too) is refused
      before any connection, with an error naming the host and address.
- [ ] The connection uses exactly the addresses that were checked.
- [ ] SEP-1 and NFT metadata fetches use the guard; RPC calls are unchanged.
- [ ] Tests in their own files; cargo test / clippy / fmt green;
      cargo-mutants over the diff with every survivor answered.
- [ ] **Docs updated** — `N/A — no change to schema, endpoints, pipeline or
    topology; an internal check on outbound requests`.
- [ ] **API types regenerated** — run (Cargo.lock changes); diff expected empty.

## Notes

- IP literals never reach a `Resolve` (hyper skips DNS for them); both clients
  already refuse IP-literal hosts and redirect targets (`validate_host`,
  `validate_uri`), so the two checks together cover every host form.
