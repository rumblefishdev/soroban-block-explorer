---
prefix: S
title: 'NFT coverage — per-question verdicts, gap list, decisions'
status: mature
spawned_from: '0391'
date: 2026-07-14
who: karolkow
---

# S — NFT coverage: verdicts + concrete gaps + decisions

Synthesis over [[R-nft-coverage-measured-state]]. Bottom line: **0383 left no
NFT hole on the account side; the only real gap is NFT-page visibility, which is
already owned by existing tasks.** No new NFT decode work.

## Per-question verdict (0383 devil's-advocate Q1–Q4)

### Q1 — `consecutive_mint` not matched by 0383's parser → **NOT a gap**

Two reasons, either sufficient:

1. **Volume:** 23 events / 8 contracts chain-wide. Immaterial.
2. **Already covered:** the pre-existing NFT-owner participant path
   (`stage.rs:599`, Path B) registers the recipient — verified present in
   `transaction_participants` on a real tx (R §3d).

`consecutive_mint` also has no SEP-11 asset string (bespoke NFT) → it would be
`EventAsset::Contract` → `asset_id=None` on the asset side anyway, and NFTs are
deliberately out of the `operation_asset_appearances` scope. So nothing is lost.

### Q2 — NFT classification undercount → **REAL, but mostly false-positive noise**

`contract_type=Nft` is genuinely a floor, not a ceiling: it needs the contract's
WASM interface to expose `owner_of`/`token_uri`/… and to have been observed.
Bespoke NFTs without those names, or with un-observed WASM, land in `Other`.

But the pending pool is **not** "hidden NFTs" wholesale. Broken down (R §3b):

- 350 contracts / 161,559 tokens are **Fungible** false-positives → correctly
  invisible as NFTs; `nft_reclassify` drops them.
- 21 contracts / 429 tokens are **Nft**, classified, merely **unpromoted**.
- 423 contracts / 14,632 tokens are **Other** — the genuine unclassifiable
  residual.

So the undercount that actually costs NFT-page coverage is 429 (fixable now) +
14,632 (needs WASM). Not 176k.

### Q3 — collection/asset page completeness → **gap = un-promoted + unclassifiable**

NFT pages read the HOT tables only (R §1b). Therefore:

- The 21 `Nft`-classified-but-pending collections: their `/v1/nfts` listing,
  detail, and per-token `/transfers` (from `nft_ownership`) are **empty until
  promoted**. The data EXISTS in `*_pending` — this is a drain/latency gap, not
  a decode gap.
- The 423 `Other` collections: unresolvable without WASM. Same as any
  unclassified contract.

**Not analogous to 0383's fungible fix.** 0383 had to _write a brand-new asset
index_ because the op path emitted nothing for an `InvokeHostFunction`. Here the
NFT rows are already parsed and staged (into `*_pending`); the shortfall is
promotion + classification, both pre-existing machinery.

### Q4 — account-page NFT coverage → **SOLID (classification-independent)**

Standard verbs get both sides (Path A once 0383 deploys; Path B for `to`);
`consecutive_mint` recipient via Path B. Because both writers ignore
`contract_type`, even quarantined (`Other`) NFTs' movers appear on account
pages — the account sees the tx (generic op tag), just not NFT-typed rendering.

## Concrete gaps → owning tasks (nothing new to decode)

| #   | Gap                                                                              | Size                              | Fix / owner                                                                                                              |
| --- | -------------------------------------------------------------------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| 1   | 21 `Nft` collections un-promoted; 350 fungible false-positives polluting pending | 429 tokens promote + 161,559 drop | Run `nft_reclassify` drain — **task 0217** runbook `docs/runbooks/0217_nfts_pending_migration_and_drain.md`. Mechanical. |
| 2   | Promotion is a manual Phase-3 pass, not live → inherent NFT-page staleness       | structural                        | Schedule the drain (recurring). **Candidate new OPS task** if 0217 doesn't own the recurring cadence.                    |
| 3   | 423 `Other` collections unclassifiable (WASM not observed)                       | 14,632 tokens                     | **0309** (classifier design), **0320** (WASM-upgrade reclassify), **0316** (WASM observation). Not new work.             |
| 4   | `parse_token_event` doesn't match `consecutive_mint`                             | 23 events                         | Optional parser-symmetry add; **zero** coverage impact (Path B covers it). Low priority.                                 |

## Decisions

### From Plan

1. **Audit only; do not duplicate 0383.** The prompt scoped this as
   audit + gap-closure that must not re-do fungible work. Confirmed the NFT
   account-side is already covered — so the deliverable is the measurement +
   gap map, not code.

### Emerged

2. **Reframed the "176k quarantined" alarm.** First-glance read of
   `nfts_pending` size suggests a massive NFT gap; the verdict breakdown shows
   91% is fungible false-positives. Documented the breakdown so a future session
   doesn't chase a 176k-token ghost.
3. **Did not spawn new sub-tasks for gaps 1/3.** They map cleanly onto existing
   tasks (0217/0309/0320/0316); spawning duplicates would create lore-id churn.
   Gap 2 (recurring-drain scheduling) is the only genuinely-unowned item —
   flagged as a _candidate_ task rather than auto-created, pending user call on
   whether 0217 already owns cadence.
4. **Declared 0359 irrelevant to NFTs.** The 0383 handoff flagged
   `operation_asset_appearances` (0359) as a deploy dependency; verified no NFT
   endpoint reads it, so it gates nothing here. Removes a false blocker.
5. **Classification-independence of participant writes is the load-bearing
   fact.** It's why Q4 is solid and why Q2's undercount does _not_ leak into
   account-page coverage. Called out explicitly because it's non-obvious (one
   would expect the NFT classifier to gate NFT participant rows — it doesn't).
