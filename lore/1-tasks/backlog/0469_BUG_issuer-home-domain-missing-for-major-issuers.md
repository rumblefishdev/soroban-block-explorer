---
id: '0469'
title: 'BUG: issuer home_domain is null for major issuers that carry one on-chain (USDC, AQUA)'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0450', '0188']
tags: [indexer, data-quality, assets, priority-high, effort-medium]
links: []
history:
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0450 audit. The assets-list issuer-domain column ships
      and renders correctly, but is empty for the two largest classic assets
      on the network. Verified against the chain: the USDC issuer carries
      `circle.com` and the AQUA issuer `aqua.network`, while our API returns
      `issuer_home_domain: null` for both. Coverage across classic assets is
      22.2 %. A whole-row RMT clobber was suspected and refuted by
      measurement — this is missing capture, not overwriting.
---

# BUG: `home_domain` never captured for issuers that do not transact

## Summary

`accounts.home_domain` is null for issuers that demonstrably carry one on the
ledger. The assets-list column added by [[0450]] therefore renders blank for
the very assets it was built to serve.

## Evidence

| Asset                 | Issuer          | On-chain `home_domain` | Our API |
| --------------------- | --------------- | ---------------------- | ------- |
| USDC                  | `GA5ZSEJY…KZVN` | `circle.com`           | `null`  |
| AQUA (61 863 holders) | `GBNZILST…AQUA` | `aqua.network`         | `null`  |

On-chain values read from Horizon's account endpoint — a raw ledger-entry
field, not a derived one, so the usual caution about validating against a
source that re-derives the field does not apply here.

Coverage: **75 288 of 338 963 classic assets (22.2 %)** resolve an issuer
domain. Across all accounts, 1 014 072 carry one.

## Refuted hypothesis — record it, do not re-open

The `accounts` table is a ReplacingMergeTree and its own schema comment warns
that reads must take the latest version, so a whole-row clobber looked like
the obvious cause: a batch that sees the account only as a participant writes
a fresh row with a bumped `last_seen_ledger` and a null domain, and the newest
row wins.

**Measured and refuted.** Of the 1 014 072 accounts that ever carried a
domain, **zero** lose it on their newest row. The value is never overwritten;
it is never written in the first place.

## Leading hypothesis (unproven)

`extract_account_states` reads `home_domain` from the account entry inside
ledger-entry changes (`crates/xdr-parser/src/state.rs:474`). An issuer's own
`AccountEntry` appears in those changes only when the issuer's own account is
modified — issuing an asset does not touch it, the holder's trustline does.
An issuer that set its domain before our indexing window and has not submitted
a transaction since would therefore never be seen, while `last_seen_ledger`
still advances through the participant path.

This explains both the null and the 22.2 % coverage, but it has not been
proven. Establish it before building anything.

## Implementation sketch

If the hypothesis holds, the value cannot come from the change stream alone —
it needs the account's current state. Options to weigh, not a decision:

- seed from a ledger-entry snapshot for accounts first seen before the window
- resolve on demand for issuers only (a bounded set) via the same runtime
  enrichment path that already fetches SEP-1 documents (task 0188)
- accept the gap and make it legible rather than silent

Whatever is chosen, a blank column must not read as "this issuer has no
domain" when the truth is "we never looked".

## Acceptance criteria

- [ ] Root cause established and recorded — hypothesis confirmed or replaced
- [ ] USDC and AQUA resolve their real domains, or the UI states plainly that
      the value is unknown rather than absent
- [ ] Coverage measured before and after, on the same population
- [ ] No regression in the 22.2 % that already resolve
- [ ] **Docs updated** — the accounts/assets contract under
      `docs/architecture/**` states where `home_domain` comes from and what a
      blank means
- [ ] **API types regenerated** — if the wire shape changes
