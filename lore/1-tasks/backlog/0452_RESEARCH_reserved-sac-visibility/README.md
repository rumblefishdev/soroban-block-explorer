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

## Step 1 — the measurement that decides it

Everything below hinges on one ratio nobody has looked at:

```sql
SELECT countIf(sac_deployed = 0) AS reserved,
       countIf(sac_deployed = 1) AS deployed
FROM asset_sac
WHERE sac_contract_id != 0;
```

And, for the denominator, how many classic assets exist at all:

```sql
SELECT count() FROM assets WHERE asset_type IN (0, 1);
```

Read it as: **of the classic assets that have moved, how many have a real
contract?** If reserved handles dominate, the row is mostly noise and option C
looks weak. If they are rare, the whole question is marginal and the cheapest
option wins.

## The three answers

**A — derive always, show always with status.** Every classic asset gets the
row; `sac_deployed` becomes a status rather than a gate on visibility. Nothing
is arbitrary and nothing is hidden. Cost: most rows show an address where
nothing is deployed and probably never will be — consistency bought with noise.
Backend change is one line (drop the `sac_contract_surrogate != 0` gate at
`crates/api/src/assets/handlers.rs:72`).

**B — show only deployed.** Consistent and quiet, and matches what every other
surface already means by "SAC" (the `Has SAC` filter and the `SAC` chip both
key off `sac_deployed`). Cost: loses the signal that a reserved address exists —
which was the reason the 0450 attempt at this was reverted.

**C — leave it, explain it.** Cheapest, and keeps the oddity visible. Cost:
requires explaining CAP-67 event attribution inside a table cell, which is a
lot to ask of a caption.

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

- [ ] The ratio above is measured and recorded here
- [ ] One of A / B / C chosen, with the reason and the number behind it
- [ ] A follow-up implementation task spawned (or this closed as "leave it")
- [ ] Replacement wording drafted for the "Reserved address" caption
