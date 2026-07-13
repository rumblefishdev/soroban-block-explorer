---
title: 'External cross-validation — Horizon, Hubble, stellar.expert, indexers'
type: research
status: mature
spawned_from: '0359'
spawns: []
tags: ['cross-validation', 'stellar', 'horizon', 'hubble']
links: []
history:
  - date: 2026-07-07
    status: mature
    who: karolkow
    note: 'Cross-checked the design against mature Stellar data systems (Horizon, Hubble/stellar-etl, stellar.expert, indexers) + recorded the hard completeness facts. Extracted from the 0359 README on folder conversion.'
---

# External cross-validation & completeness ceiling

Cross-checks the design against mature Stellar data systems and records the hard
completeness facts (max assets per op, the two-grain endpoint/trade split).

## Completeness ceiling & external references

### The real ceiling: one operation can touch up to 7 assets

A `PathPaymentStrict{Send,Receive}` op references **sendAsset + up to 5
intermediate `path` hops + destAsset = up to 7 distinct assets** (Stellar
`MAX_PATH_LENGTH = 5`, XDR `Asset path<5>`). The parser already emits all of them
(`operation.rs:224/235` writes the full `path[]` to details JSON); the projection
keeps one (`stage.rs:1757`). So:

- 2 slots (Stanisław) = endpoints only → drops every routing hop. Incomplete.
- 3 slots = still < 7 → drops hops on longer paths. Incomplete.
- **Any fixed N is a scope-narrowing hack.** Asset participation is genuinely
  many-to-many with N up to 7 → only a variable-cardinality model is complete.

### External cross-validation (2026-07-07) — Horizon + stellar.expert

- **Canonical Stellar/Horizon model treats path hops as first-class assets.** The
  Horizon path-payment operation object exposes `source_asset`, (destination)
  `asset`, AND `path[]` = _"The intermediary assets that this path hops through."_
  Their own doc example = **4 distinct assets** (USD source, BRL dest, EURT + XLM
  hops). → indexing only endpoints is a KNOWN-INCOMPLETE view of the authoritative
  data. Complete/correct data MUST include the hops.
- **Horizon cannot filter operations/payments by asset for ANY asset** (no asset
  param on `/payments` or `/operations`; only `/trades` has base/counter). → task
  claim CONFIRMED; our per-asset CH index is a capability Horizon lacks.
- **stellar.expert's _documented public API_ has NO per-asset operations/history
  endpoint** — only holders / supply / rating / metadata. The per-asset tx history
  the task references (`/explorer/public/tx?asset[]=XLM`) is their **web-UI search
  backed by a private internal index**, not a public API (correction to the task's
  "exposes via its own per-asset index" wording). Still proves the feature is
  buildable — they built it internally — but it is NOT externally queryable, so we
  can't diff against it via API; validation must use raw XDR + Horizon per-op.

### More external sources (2026-07-07, round 2) — Hubble/stellar-etl + indexers

Broadened beyond Horizon + stellar.expert at karolkow's request:

- **Hubble / stellar-etl (SDF's OWN canonical warehouse, BigQuery `crypto-stellar`).**
  `enriched_history_operations` flattens **`source_asset_{code,issuer,type}` +
  `asset_{code,issuer,type}` (destination)** to columns = a **2-endpoint-slot**
  model, and keeps the intermediate path as a **nested `details.path` record
  array** ("the intermediary assets that this path hops through will be reported
  in the record") — queryable only via UNNEST, **NOT a first-class per-asset
  participation**. So the reference warehouse indexes endpoints as columns and
  treats hops as metadata; it does not model hop assets as operation participants.
- **Indexers (Mercury, SubQuery, Goldsky).** Mercury → Postgres: ledgers / tx /
  ops / effects + Soroban events. SubQuery: indexes **Soroban transfer events** +
  account payments (credits/debits). Goldsky: no Stellar subgraphs (EVM-only),
  Stellar via Turbo pipelines. → the standard way to make Soroban token flow
  queryable is decoding the **events** — corroborates the L2 workstream
  (transfer/mint/burn decode) as industry-standard, not novel.
- **stellarchain.io / LOBSTR** — UI explorers (live feeds, asset/DEX analytics,
  account history); no documented per-asset-participation API to diff against.

**Refinement this forces on the design (important):** the ecosystem consensus
(Horizon **and** Hubble) models path-payment participation as **endpoints
(source + dest)** at the operation grain, with the **path as metadata**. Hop-asset
activity is captured as **trades (result `ClaimAtom`s)**, not as the parent
operation. So:

- Attributing the parent path-payment op to its hop assets (the "up to 7" framing)
  is **beyond-reference** AND **redundant** with a complete trades/claim-atom
  stream — a hop asset already surfaces the crossing as a trade. Per karolkow's
  "chyba że coś jest zbędne", op→hop attribution is the redundant part; drop it.
- The genuinely complete model = **two participation grains, unioned**:
  1. **Operation participation** — declared legs: payment asset; path-payment
     source+dest; offer selling+buying; trustline/clawback/claimable asset;
     LP-deposit/withdraw pool legs. (Bounded, ≤2 per classic op.)
  2. **Trade participation** — one per **result ClaimAtom** (order-book + LP
     crossings), each carrying its two traded assets. **This is the unbounded-N,
     result-authoritative stream** — a single path-payment/offer can cross many
     offers → many claim atoms → **>2 (dozens) asset-participations per op**.

**This is the real kill-shot for any fixed-N slot:** even ignoring the declared
path, the **result side alone produces an unbounded number of (asset) trade
participations per op** (up to the ops/tx resource limit's worth of crossed
offers). No fixed 2-/3-/7-column scheme can hold it. → **fan-out is mandatory**,
confirmed independently of the declared-path argument.
