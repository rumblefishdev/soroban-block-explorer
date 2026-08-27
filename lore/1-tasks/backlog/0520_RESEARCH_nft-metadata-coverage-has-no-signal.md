---
id: '0520'
title: 'RESEARCH: one in five NFTs has no metadata, and nothing distinguishes "nothing to fetch" from "fetch is failing"'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0455', '0311', '0392']
tags: ['observability', 'enrichment', 'nft', 'effort-small', 'priority-medium']
links: []
history:
  - date: 2026-08-27
    status: backlog
    who: karolkow
    note: >
      Spawned from 0455's carried-items list. The umbrella's defect 2 is
      "health measured by success"; this is that defect applied to an
      enrichment family - the worker reports success, a fifth of promoted NFTs
      carry nothing, and no signal separates the legitimate residual from a
      silent failure. Measured before filing rather than estimated.
---

# RESEARCH: NFT metadata coverage has no signal

## Summary

**79.3% of promoted NFTs carry a name or a media URL. The other 2 849 carry
neither, and nothing in the system says whether that is correct.**

The residual may be entirely legitimate — a contract with no `token_uri` has
nothing to fetch and never will. It may also be a dead gateway, a malformed
URI shape, or a fetch that fails the same way every time. Today those look
identical from the outside: the worker acks, the DLQ stays empty, and the
number is only visible to whoever runs the query by hand.

## Measured 2026-08-27 (production)

|                              |                   |
| ---------------------------- | ----------------- |
| Promoted NFTs (`nfts FINAL`) | 13 752            |
| With a name                  | 10 899            |
| With a media URL             | 10 903            |
| **With neither**             | **2 849 (20.7%)** |

**The gap is all-or-nothing per collection, which is the finding.** Of the 58
collections with any missing token:

- **51 collections are 100% missing** — 2 716 tokens
- **7 collections are partially missing** — 133 tokens

A per-token fetch failure would scatter. A whole collection missing points at
one cause per collection: no `token_uri` on the contract, a URI shape the
fetcher cannot resolve, or a gateway that fails for that collection's CIDs.
**The 7 partial collections are the interesting ones** — same contract, same
URI shape, different outcome per token.

Method note (recorded because it already produced a false answer once): the
`name` / `media_url` columns on `nfts` itself are vestigial NULL by design —
the live indexer rewrites that row on every ownership change — so counting
over `nfts.name` reads 0% and means nothing. Join to `nft_enrichment` and take
`argMax(_, version)` per `(contract_id, token_id)`.

## What to establish, in order

1. **Split the 51 all-missing collections by cause.** For a sample: does the
   contract expose a `token_uri` at all? If yes, what shape (ipfs://, https://,
   data:, something else)? Does a manual fetch succeed?
2. **Explain the 7 partial collections.** Same contract, differing outcome —
   this is where a real fetch defect would hide.
3. **Only then decide whether a signal is worth it.** Options, cheapest first:
   a coverage figure in the health runbook's diagnosis queries (no alarm); a
   dashboard series; an alarm on coverage dropping below a floor. An alarm is
   probably wrong — coverage moves slowly and a level alarm on it would latch,
   which ADR 0054 rule 2 exists to prevent.

## Note on the redirect change (lore-0455, 2026-08-27)

The NFT fetcher moved from `Policy::limited(0)` to the shared
same-registrable-domain policy the same day this was measured. A directory CID
without a trailing slash used to lose its content to a refused `301`; it no
longer does. **Re-run the coverage query after that ships** — some of the 2 849
may resolve themselves, and the number above is the pre-change baseline.

## Acceptance Criteria

- [ ] The 2 849 split into "nothing to fetch" vs "fetch fails", with the method
      recorded so the split can be re-derived
- [ ] The 7 partial collections explained
- [ ] Coverage re-measured after the redirect-policy change is deployed, and
      the delta attributed
- [ ] A decision on the signal — runbook query, dashboard series, alarm or
      nothing — with the reason written down
- [ ] **Docs updated** — if a signal is adopted, `docs/runbooks/health.md`
      coverage matrix gains its row; N/A if the answer is "nothing"
- [ ] **API types regenerated** — N/A, no API surface change
