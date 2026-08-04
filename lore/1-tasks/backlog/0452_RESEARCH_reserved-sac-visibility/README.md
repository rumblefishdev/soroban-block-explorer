---
id: '0452'
title: 'RESEARCH: what should we show for a SAC address nobody deployed — measure first, then decide'
type: RESEARCH
status: backlog
related_adr: ['0051']
related_tasks: ['0323', '0337', '0339', '0450']
tags: [research, assets, sac, frontend, api, priority-low, effort-small]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Deferred out of 0450, which briefly hid the "SAC contract" row for
      un-deployed SACs and had that reverted on review — hiding an honest row
      was the lossy fix.
  - date: '2026-07-29'
    status: backlog
    who: karolkow
    note: >
      Rewritten FEATURE → RESEARCH. The first version prescribed "derive the
      address for every classic asset", which is one of three defensible answers
      and was written as though it were settled. It is not: making the row
      unconditional trades "appears arbitrarily" for "always present, almost
      always empty", and nobody has checked which is worse. Converted to a
      directory per the RESEARCH convention; the deciding measurement is now
      step 1.
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Step 1 measured on mainnet: 3 804 deployed vs 296 437 reserved — 1.3%.
      The premise both earlier versions rested on is refuted. The row does not
      appear arbitrarily; it already renders on ~89% of classic asset pages and
      says "not deployed" on 98.7% of them. So it is not a signal, it is the
      default state, and hiding it loses a constant rather than information.
      Recommendation recorded (B, address kept reachable as an explicitly
      derived value); the choice itself is still open.
---

# RESEARCH: what to show for a SAC address nobody deployed

## The question

The asset detail page shows a `SAC contract` row with an address and
"Reserved address — not deployed" for some classic assets and not others, with
nothing on screen explaining the difference. **What should it show instead —
and for which assets?**

Not "how do we implement X". The implementation is trivial either way; the
open question is which behaviour is right.

## Why the row is inconsistent today

Two facts, both verified:

1. **Every classic asset has a SAC address.** It is derived from
   `(asset_code, issuer, network)` — `xdr_parser::derive_sac_strkey`
   (`crates/xdr-parser/src/sac.rs:105`) is pure computation, no lookup. Nobody
   reserves it; it simply is where that asset's contract would live.
2. **We only learn of one when the asset emits a CAP-67 unified asset event.**
   Since CAP-67 an ordinary classic transfer emits `transfer` / `mint` / `burn`
   / `clawback` / `set_authorized` under that derived address whether or not a
   contract was ever deployed (`crates/xdr-parser/src/sac.rs:189-190`), and
   `detect_undeployed_sac_overrides` records the handle with
   `sac_deployed = false` (task 0323).

So `sac_contract_id != 0` means **"this asset has moved"**, not "this asset has
a contract". Seen live: `zyx` (supply 0, never moved) shows no row; `zxc`
(minted, one holder) shows an un-deployed address. Same asset class, same
absence of any contract, different UI.

The address in that second row is genuinely not on chain — confirmed for
`CC774ZITP2FCKQ3RACDQPZKCQXXFNJBSNG4VJ6PDNEI4REO6EZCEUP67` by a hand-built
`getLedgerEntries` call (`entries: []`) and by stellar.expert (404).

## Step 1 — the measurement that decides it — DONE

Measured on mainnet 2026-07-30.

|                                     |       count |          |
| ----------------------------------- | ----------: | -------: |
| reserved (`sac_deployed = 0`)       | **296 437** |    98.7% |
| deployed (`sac_deployed = 1`)       |   **3 804** | **1.3%** |
| rows in `asset_sac` (one per asset) |     300 241 |          |
| classic assets in total             |     337 705 |          |

```sql
-- Reserved vs deployed. The GROUP BY is not optional: asset_sac is an
-- AggregatingMergeTree, so a raw countIf counts PARTS, not assets.
SELECT countIf(sac_deployed = 0) AS reserved,
       countIf(sac_deployed = 1) AS deployed,
       count()                   AS total
FROM (
    SELECT max(sac_deployed) AS sac_deployed
    FROM asset_sac
    GROUP BY asset_type, asset_code, issuer_id, contract_id
);

-- Denominator.
SELECT count() FROM (
    SELECT 1 FROM assets WHERE asset_type IN (0, 1)
    GROUP BY asset_type, asset_code, issuer_id, contract_id
);
```

The original `WHERE sac_contract_id != 0` predicate was dropped — every row in
the table carries a non-zero surrogate, so it filtered nothing.

### What the number means

**Of the classic assets that have ever moved, 1.3% have a real contract.**

And the second ratio reframes the whole question: 300 241 of 337 705 classic
assets already have an `asset_sac` row, so **the row this task is about already
renders on ~89% of asset detail pages — and on 98.7% of those it says "reserved,
not deployed".**

So the premise that the row appears "arbitrarily" was wrong. It appears almost
always, and almost always says the same thing. **A signal that fires on 98.7% of
cases carries no information.** That is the finding, and it is what decides
between A, B and C below.

## The three answers — re-read against the number

**A — derive always, show always with status.** Every classic asset gets the
row; `sac_deployed` becomes a status rather than a gate on visibility. Nothing
is arbitrary and nothing is hidden. Backend change is one line (drop the
`sac_contract_surrogate != 0` gate at `crates/api/src/assets/handlers.rs:72`).

> **After the measurement:** this is barely a change — the row already shows on
> ~89% of pages, and A takes it to 100%. It buys consistency at the price of
> 98.7% of asset pages carrying an address that points at nothing. The cost was
> stated as a guess; it is now known and it is the whole population.

**B — show only deployed.** Consistent and quiet, and matches what every other
surface already means by "SAC" (the `Has SAC` filter and the `SAC` chip both
key off `sac_deployed`). Cost: loses the signal that a reserved address exists —
which was the reason the 0450 attempt at this was reverted.

> **After the measurement:** the objection weakens sharply. The "signal" fires
> on 98.7% of assets, so it does not distinguish anything — it is the default
> state of every classic asset that ever moved. B would drop the row from ~89%
> of pages to 1.3%, and what it drops is a constant. It stays lossy only if
> someone genuinely needs the derivable address on a page where no contract
> exists — and if they do, that is a different feature (show the address as a
> derived fact, not as a deployment).

**C — leave it, explain it.** Cheapest, and keeps the oddity visible. Cost:
requires explaining CAP-67 event attribution inside a table cell, which is a
lot to ask of a caption.

> **After the measurement:** there is no oddity to keep visible. C now means
> "explain, on 89% of pages, why a near-universal row is near-universally
> empty". Cheapest to build, hardest to word.

**Recommendation: B, with the address kept reachable rather than deleted** — the
row disappears from pages where nothing is deployed, and the derivable address,
if wanted at all, returns as an explicitly-labelled derived value rather than as
something that looks like a deployment. Decide it; do not let it default.

Whichever wins, **"Reserved address — not deployed" needs rewording** — nobody
reserved anything.

## Constraints on any answer

- Do not change what `sac_deployed` means, and do not let this leak into the
  `Has SAC` filter or the `SAC` chip. Both correctly mean "deployed"; 0450
  established that all surfaces should agree on that.
- **Soroban-native assets have no SAC** — the contract IS the asset. No
  derivation for `asset_type = 3`.
- **Native XLM** has its own SAC and `derive_sac_strkey` handles the empty
  code/issuer pair; check it renders sensibly rather than as a classic asset.
- The assets **list** deliberately gave that column back to the issuer (0450).
  Whatever wins here is about the detail page unless there is a reason to
  revisit that.

## Done when

- [x] The ratio above is measured and recorded here — 1.3% deployed
- [ ] One of A / B / C chosen, with the reason and the number behind it
- [ ] A follow-up implementation task spawned (or this closed as "leave it")
- [ ] Replacement wording drafted for the "Reserved address" caption
