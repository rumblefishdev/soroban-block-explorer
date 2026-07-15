---
prefix: R
title: 'NFT token-flow coverage — measured prod state + code trace'
status: mature
spawned_from: '0391'
date: 2026-07-14
who: karolkow
---

# R — NFT coverage: what the code does + what prod measures

All measurements: prod ClickHouse via `chq`, 2026-07-14. Code refs are live
on-disk line numbers on branch `feat/0383_...` (which == develop for the NFT
paths audited here — the NFT participant path and read paths predate 0383).

## 1. How an NFT movement reaches each surface

### 1a. Account page (activity/tx list) ← `transaction_participants`

`GET /v1/accounts/{id}/transactions` seeks `transaction_participants` by the
account surrogate (`accounts/queries.rs:505`; table
`ORDER BY (account_id, ledger_sequence, transaction_id)`). It does NOT read
`operation_asset_appearances`, `nft_ownership`, or invocations for the list.

**Two independent writers put NFT participants into `transaction_participants`
(`stage.rs`), and BOTH ignore contract classification:**

| Path                                                 | Location                                                             | Verbs matched                                 | Sides registered                                 |
| ---------------------------------------------------- | -------------------------------------------------------------------- | --------------------------------------------- | ------------------------------------------------ |
| **A** — `derive_token_event` (task 0383)             | `stage.rs:530-543` → `parse_token_event` (`event_filters.rs:57-105`) | transfer / mint / burn / clawback             | **both** `from` + `to`                           |
| **B** — dedicated `nft_events` owner loop (pre-0383) | `stage.rs:599-609`                                                   | transfer / mint / burn / **consecutive_mint** | **`to` only** (`owner_account`; `None` for burn) |

`owner_account` = `to` for Mint|Transfer, `None` for Burn
(`state.rs:1288-1291`). `NftEvent.from` is dropped before `ExtractedNftEvent`
(`types.rs:471-489`) — so Path B never carries the previous owner. Path A
supplies it.

Combined per verb:

| verb                 | `from` covered by      | `to` covered by                          |
| -------------------- | ---------------------- | ---------------------------------------- |
| transfer             | Path A (0383)          | Path A + Path B                          |
| mint                 | — (mint has no `from`) | Path A + Path B                          |
| burn                 | Path A (0383)          | — (burn has no `to`)                     |
| clawback             | Path A (0383)          | Path A (0383)                            |
| **consecutive_mint** | — (no `from`)          | **Path B only** (0383's parser skips it) |

Consequence: participant coverage is **classification-independent**. Even a
contract mis-classified `Other` (quarantined in `*_pending`) still has its NFT
movers registered, because Path A/B iterate the raw detected events, not the
routed rows.

### 1b. NFT collection / detail / per-token pages ← HOT tables only

| Endpoint                                              | Table seeked        | Key                                                                          |
| ----------------------------------------------------- | ------------------- | ---------------------------------------------------------------------------- |
| `GET /v1/nfts` (list/collection)                      | `nfts` FINAL        | keyset `(minted_at_ledger, contract_id, token_id)`; optional contract filter |
| `GET /v1/nfts/{c}/{t}` (detail)                       | `nfts` FINAL        | `(contract_id surrogate, token_id)`                                          |
| `GET /v1/nfts/{c}/{t}/transfers` (per-token activity) | **`nft_ownership`** | `(contract_id surrogate, token_id)`                                          |

None read `*_pending`, `operation_asset_appearances` (0359),
`transaction_participants`, or `soroban_invocations_appearances`. So an NFT is
visible on these pages **iff its rows are in the HOT `nfts`/`nft_ownership`
tables** — i.e. iff its contract is `Nft`-classified AND promoted.

### 1c. Contract detail invocations ← `soroban_invocations_appearances` (arm B)

`GET /v1/contracts/{c}/invocations` seeks `soroban_invocations_appearances` by
contract surrogate (`contracts/queries.rs:711`). This is the contract page, not
the NFT collection page — it lists the contract's invocations, not per-token NFT
movement.

## 2. Classification + routing + promotion

- **Classifier** `classify_contract_from_wasm_spec` (`classification.rs:101`):
  reads WASM interface function names. `owner_of|token_uri|approve_for_all|
get_approved|is_approved_for_all` → **Nft**; else `decimals|allowance|
total_supply` → **Fungible**; else **Other**. NFT wins on dual interface
  (deliberate false-positive bias). **Only runs when a contract's WASM is
  observed.** WASM never observed → contract stays `Other`/NULL. `Other` is
  never cached as definitive (re-queried each time).
- **Routing** `route_for` (`stage.rs:1411-1423`): `Nft → Hot`
  (`nfts`/`nft_ownership`); `Fungible|Token → Drop`; `Other|NULL|uncached →
Pending` (`nfts_pending`/`nft_ownership_pending`).
- **Promotion** `nft_reclassify.rs` (backfill-runner, **Phase-3 post-merge
  maintenance — NOT live**): `INSERT INTO nfts SELECT … FROM nfts_pending WHERE
contract_id IN (soroban_contracts FINAL WHERE contract_type=2)`; DELETEs
  `Fungible|Token` from pending; cleans legacy `Fungible|Token` from hot.
  Promotion happens only after `contract_type_rebuild` upgrades the verdict to
  `Nft` — i.e. after the WASM finally becomes visible.
- **Feeder** `nft_reparse.rs`: re-runs `detect_nft_events` over already-stored
  `soroban_events` typed-JSON (recovers dropped shapes incl. `consecutive_mint`,
  no S3 re-ingest) → writes **PENDING only**.

## 3. Prod measurements (2026-07-14)

### 3a. Hot vs pending

| table                                             | contracts | rows    |
| ------------------------------------------------- | --------- | ------- |
| `nfts` (hot, on pages)                            | 60        | 12,835  |
| `nfts_pending` (quarantine, never read by API)    | 794       | 176,604 |
| `nft_ownership` (hot)                             | 60        | 21,312  |
| `nft_ownership_pending`                           | 794       | 315,506 |
| `soroban_contracts` where `contract_type=2` (Nft) | 122       | —       |
| `soroban_contracts` total                         | 180,704   | —       |

### 3b. `nfts_pending` broken down by the contract's current verdict

| verdict          | contracts | token rows | interpretation                                                                                                                                         |
| ---------------- | --------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Fungible (3)** | 350       | 161,559    | **false positives** — permissive i128 shape parser flagged fungible transfers; `nft_reclassify` DROPs them. Not NFTs; correctly absent from NFT pages. |
| **Other (1)**    | 423       | 14,632     | **genuine unknowns** — WASM not observed → unclassifiable. The real residual.                                                                          |
| **Nft (2)**      | 21        | 429        | **classified but unpromoted** — promotion-lag; drain promotes → pages.                                                                                 |

Reconciliation: 122 `Nft`-classified contracts, but hot `nfts` has 60 + pending
has 21 = 81. The other ~41 `Nft`-classified contracts have zero token rows in
either table (classified from WASM but no minted-token event surfaced, or all
rows already dropped). Not a coverage loss — nothing to show.

**Key reframing:** the headline "176,604 quarantined tokens" is misleading —
**91% (161,559) are fungible false-positives** that should never appear as NFTs.
True NFT-page shortfall = 429 (promotion-lag, mechanical) + 14,632 (Other,
hard).

### 3c. Verb distribution in `soroban_events` (RMT pre-merge counts; ratios hold)

| signature            | events | contracts |
| -------------------- | ------ | --------- |
| transfer             | 7.20 B | 206,052   |
| mint                 | 545 M  | 186,896   |
| clawback             | 97.9 M | 6,358     |
| burn                 | 84.1 M | 129,656   |
| **consecutive_mint** | **23** | **8**     |

`consecutive_mint` is negligible — the one verb 0383's parser skips is ~23
events chain-wide.

### 3d. `consecutive_mint` recipient IS a participant (Q1 empirical check)

Most-recent `consecutive_mint`: contract surrogate `5236763530277713147`
(`CAKSC7JH…`), tx `8072207398204103224`, ledger `61787807`, recipient
`GBWHGYD5DFPQMJSUEEA77IT7YJ75PYQQFOCMP7HT5OIF2ULKJK22N4J4`.

```sql
SELECT tp.account_id = (SELECT id FROM accounts WHERE account_id='GBWHGYD5…')
FROM transaction_participants tp
WHERE tp.ledger_sequence=61787807 AND tp.transaction_id=8072207398204103224
-- → is_consecutive_mint_recipient: 1
```

Recipient present → Path B covers `consecutive_mint` on the account page,
independently of 0383.

## 4. Follow-up measurement (2026-07-15) — drain is 33 days stale, leak is LIVE

Re-ran the audit one day later to test the hypothesis "the backfill was
one-shot; the live flow needs a continuous fix." Confirmed on both counts.

### 4a. Hot is frozen; pending tracks the chain tip

| table                   | max ledger | max `closed_at`         |
| ----------------------- | ---------- | ----------------------- |
| `nfts` (hot)            | 62,989,407 | **2026-06-12 01:24:45** |
| `nft_ownership` (hot)   | 62,989,407 | 2026-06-12 01:24:45     |
| `nfts_pending`          | 63,485,559 | 2026-07-15 09:01        |
| `nft_ownership_pending` | 63,485,653 | 2026-07-15 09:01:38     |
| chain tip (`ledgers`)   | —          | 2026-07-15 09:03:03     |

Both hot tables stop at the **exact same** ledger `62,989,407` → a single
promotion event (the last `nft-reclassify`), **33 days ago**. Nothing has been
promoted since. Pending runs to the chain tip.

### 4b. Day-over-day growth (07-14 → 07-15)

| table                   | 07-14          | 07-15         | Δ rows |
| ----------------------- | -------------- | ------------- | ------ |
| `nfts` (hot)            | 60c / 12,835   | 60c / 12,835  | **0**  |
| `nft_ownership` (hot)   | 60c / 21,312   | 60c / 21,312  | **0**  |
| `nfts_pending`          | 794c / 176,604 | 1144c / ~329k | +~152k |
| `nft_ownership_pending` | 794c / 315,506 | 1144c / ~588k | +~273k |

Hot bit-identical after a full day; pending nearly doubled.

### 4c. What the LIVE path wrote to pending in the last 33 days

`nft_ownership_pending` rows split at the hot cutoff ledger `62,989,407`
(post-cutoff = ingested after the last drain = necessarily live-written), joined
to the contract's **current** verdict:

| verdict  | post-cutoff (live, 33d) | pre-cutoff (older) |
| -------- | ----------------------- | ------------------ |
| Fungible | **198,290 (91%)**       | 283,399            |
| Other    | 18,488 (8%)             | 94,515             |
| Nft      | 203 (0.1%)              | 558                |

≈ **6,575 pending rows/day** accrue, **91% of them Fungible-verdict**. This is
not stale reparse residue — the live pipeline is pouring fungible transfers into
NFT-pending _right now_.

### 4d. Fungible-in-pending is 100% genuine false-positives

Of the Fungible-verdict contracts sitting in `nfts_pending`, **401/401 (100%)**
are also registered as real fungible `assets` (`asset_type IN (2,3)`):

```sql
SELECT uniqExact(p.contract_id) AS fungible_verdict_pending,
       uniqExactIf(p.contract_id, a.contract_id IS NOT NULL) AS also_fungible_asset
FROM nfts_pending p
INNER JOIN (SELECT id FROM soroban_contracts FINAL WHERE contract_type=3) sc ON p.contract_id=sc.id
LEFT JOIN (SELECT DISTINCT contract_id FROM assets FINAL WHERE asset_type IN (2,3)) a ON p.contract_id=a.contract_id
-- → 401, 401
```

Every fungible-verdict pending contract is a confirmed fungible token. None are
NFTs. The i128 shape parser mislabels the transfer; the contract's verdict only
resolves to Fungible _after_ the row is already in pending, and `Fungible→Drop`
never applies retroactively.

### 4e. The 21 Nft-verdict pending collections are already in hot

```sql
SELECT count() nft_verdict_contracts, sum(tokens) tokens_stranded,
       countIf(in_hot=0) contracts_fully_invisible
FROM (SELECT p.contract_id, uniqExact(p.token_id) tokens,
             max(h.cid IS NOT NULL) in_hot
      FROM nfts_pending p
      INNER JOIN (SELECT id FROM soroban_contracts FINAL WHERE contract_type=2) sc ON p.contract_id=sc.id
      LEFT JOIN (SELECT DISTINCT contract_id cid FROM nfts) h ON p.contract_id=h.cid
      GROUP BY p.contract_id)
-- → 21, 559, 0
```

All 21 `Nft`-verdict pending contracts (**559 tokens**) already have rows in hot
`nfts` — they were promoted in the June-12 drain. So the concrete NFT-page miss
is **post-June-12 mints of already-visible collections**: known collections
showing stale token sets, not whole collections vanished.

### 4f. Root-cause — leak vs defer is NOT resolved (do not overclaim)

First pass claimed these rows are a write-time **fail-open leak** (verdict was
resolvable at ingest, the 0283 G1/G9 prefetch missed it). A devil's-advocate
crux test does **not** support that as the dominant cause. Comparing each
fungible-verdict pending row's event ledger (`minted_at_ledger`) against its
contract's `wasm_uploaded_at_ledger`:

| WASM observed vs event               | rows    | share |
| ------------------------------------ | ------- | ----- |
| **before** event (→ fail-open, H1)   | 29,909  | 11%   |
| **at/after** event (→ correct defer) | 47,083  | 17%   |
| `wasm_uploaded_at_ledger` NULL       | 198,907 | 72%   |

```sql
SELECT
  countIf(sc.wasm_uploaded_at_ledger>0 AND sc.wasm_uploaded_at_ledger <  p.minted_at_ledger) AS wasm_before,
  countIf(sc.wasm_uploaded_at_ledger>0 AND sc.wasm_uploaded_at_ledger >= p.minted_at_ledger) AS wasm_after,
  countIf(sc.wasm_uploaded_at_ledger=0) AS wasm_zero, count() AS total
FROM nfts_pending p
INNER JOIN (SELECT id, wasm_uploaded_at_ledger FROM soroban_contracts FINAL WHERE contract_type=3) sc
  ON p.contract_id = sc.id
-- → 29909, 47083, 0, 275899   (remainder = NULL upload ledger)
```

Three competing mechanisms, not yet separated:

- **H1 fail-open leak** — WASM known before event, verdict computable, prefetch
  missed it → wrongly quarantined. Only ~11% (of all) / ~39% (of timing-known).
- **H2 correct defer** — WASM observed at/after the event → could not classify
  at ingest → _correctly_ pending, awaiting drain. ~17% / ~61% of timing-known.
  **No write-time fix can prevent these.**
- **NULL (72%)** — no recorded upload ledger; uninterpretable here. Could be
  shared-WASM (uploaded once, many deploys) which would lean H1, or unindexed
  uploads which would lean H2. Not measured.

**Conclusion:** the reconcile/drain gap is real and proven (§4a–4e), but the
"write-time fail-open leak" is a **minority and unproven** as the dominant
cause — among timing-known rows the majority are correct defer. So the primary
fix is a **continuous reconcile** (promote/drop once a contract's verdict
resolves), NOT a write-time routing change. A write-time verdict-authoritative
lookup would help only the H1 slice and should be gated on first measuring the
0283 prefetch miss-rate directly. Spawned as **0392** (reconcile primary).
