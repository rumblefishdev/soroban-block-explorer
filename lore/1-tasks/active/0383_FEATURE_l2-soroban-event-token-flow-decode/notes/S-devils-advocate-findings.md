---
prefix: S
title: "Devil's-advocate findings — measured redundancy + the has_soroban scope fix"
status: mature
spawned_from: '0383'
date: 2026-07-13
who: karolkow
---

# S — Devil's-advocate review (measured, prod)

Ran an adversarial review of the whole 0383 implementation against prod CH
(`chq`). Two findings change the design; both are measured, not argued.

## Finding 1 — most token events are redundant classic echoes; keep the Soroban ~16%

Protocol 23 makes **every classic payment emit a SAC `transfer` event**, so the
`transfer` stream is 99.4% classic echoes. But that 99.4% is **transfer-only** —
across all four verbs the net-new (Soroban-context) share is **~16.4%**, and it
is dominated by contract **mint/burn** (DeFi), NOT a sliver. 10k-ledger window,
split by tx `has_soroban`:

| verb      |         total | classic (dropped, already covered) | soroban (KEPT = net-new) |
| --------- | ------------: | ---------------------------------: | -----------------------: |
| transfer  |     3,514,167 |                  3,491,611 (99.4%) |                   22,556 |
| mint      |     1,347,475 |                            724,981 |              **622,494** |
| burn      |       191,856 |                              2,251 |              **189,605** |
| clawback  |        28,560 |                             28,560 |                        0 |
| **total** | **5,082,058** |              **4,247,403 (83.6%)** |      **834,655 (16.4%)** |

So 0383 is NOT a 0.6% task — it keeps ~835k events / 10k ledgers, mostly
contract mint/burn (protocols minting LP/receipt tokens, burning on withdrawal).
Those have **no classic operation**, so they are invisible without decoding the
event. Context: Soroban is the majority of txs (18.6M vs 17.0M classic per 100k
ledgers); only ~18% of Soroban txs move tokens, the rest are oracles/governance.

The classic ones are **already covered** by the 0359 op path
(`operation_asset_appearances` + `op.counterparties`). Proven, not assumed:

- **Tx-level, all four verbs:** 68,482 classic token-event txs in a 2k window →
  **0 absent** from `transaction_participants`. Every one already registered.
- **Account-level, transfer:** decoded a 1,500-event classic sample → 670 distinct
  `(account_id, tx)` pairs → **670/670 already present**. Op path is a superset.
- **Not token movements:** `fee` (biggest event, handled via `fee_charged`),
  `approve`/`set_authorized` (allowance/control), and app events (`swap`, `trade`,
  `withdraw`, `supply`…) carry NO value themselves — the underlying move fires one
  of the 4 verbs (measured: 1087/1087 `swap` txs also have a `transfer`). So the
  4 SEP-41/CAP-67 verbs are the complete token-movement vocabulary.

**Fix (fundamental, no plaster):** scope the ingest hook AND the backfill to
`has_soroban = true`. Drops the 83.6% redundant classic echoes, keeps the 16.4%
contract-internal flows — the actual point of the task. ~99% less _transfer_
work, zero participant loss (measured).

## Finding 2 — asset-side target is not deployed (0359 not shipped)

`operation_asset_appearances` **does not exist in prod** (`EXISTS TABLE` → 0). It
is in develop's `init.sql` but 0359 has not been deployed. Consequence for 0383:

- the asset-presence write (ingest) + asset backfill target a **missing table**;
  the backfill would fail `UNKNOWN_TABLE` today.
- couldn't verify the asset superset empirically (no data), but by symmetry with
  the 670/670 participant result (`emit_asset_appearances` runs per classic op)
  it holds once the table exists + 0359 backfills it.

**Dependency:** 0383's **asset half** is downstream of 0359 being deployed AND
backfilled in prod. The **participant half** is not blocked (`transaction_participants`
is live). Sequence accordingly.

## Finding 3 — I briefly re-opened A vs B; it was wrong. A is confirmed.

After Finding 1 shrank the scale, I floated reconsidering B (a from/to/amount
table). Karol correctly pointed at 0359: it is the **same decision**, already
made. Verified in code:

- **The activity lists carry no amounts.** Both `accounts/queries.rs` and
  `assets/queries.rs` page SELECTs return only tx-header columns (`hash, ledger,
source, fee, successful, operation_count, has_soroban, created_at`) + op-type
  tags. The list path is deliberately "archive-free" (`handlers.rs:154`).
- **The tx-detail page pulls the whole tx from archive XDR** at read time (E3
  `get_transaction`, ADR 0029): envelope/result XDR + contract events (incl.
  transfer/mint/burn **amounts**) via `stellar_archive::extract_e3_heavy`.
- 0359 itself deliberately reverted a role/`leg_index`/**amount** fan-out to pure
  presence (0359 README:227-231).

So amount is needed **nowhere** in these indexes: the list never shows it, the
detail decodes it from archive on demand. B would diverge from 0359 for the same
surface AND store a column nothing renders — the plaster. **A (presence-only) is
correct and final.** The retained `amount` in the parser was dead → removed.

## Applied

1. **Scope `has_soroban = true`** (ingest `stage.rs` + backfill read query). Done.
2. **Removed dead `amount`** from `TokenEvent` / `parse_token_event` /
   `derive_token_event`. Done.
3. Asset half still sequences after 0359 deploy + backfill (dependency, not a bug).

## Second review pass (4 parallel agents: review / simplify / devil / requirements)

Consensus: implementation fundamentally sound (0 Critical/High correctness bugs,
no secrets, no over-engineering, good error handling, spec↔docs↔code consistent).
Applied all actionable findings:

- **rustfmt** — the diff was not fmt-clean (CI gate). `cargo fmt` applied.
- **Dead legacy transfer cluster removed** (~200 LOC): `parse_transfer`,
  `transfer_participants`, `is_transfer_event`, `parse_transfer_shape`,
  `Transfer`, `numeric_scval` + their tests + `lib.rs` exports. Grep-proven 0
  callers (the 2 "parse_transfer" hits were `try_parse_transfer` in nft.rs).
- **`event_asset_id` collapsed into `derive_token_event`**; dropped the dead
  `contract_strkey` param + the `token_event_asset_tests` module (coverage kept
  by `derive_token_event_tests`).
- **Backfill read reworked to PREWHERE** (`signature`/`ledger`/`transaction_id IN
(soroban tx ids)`): materializes the heavy `topics_xdr` only for `has_soroban`
  survivors. Measured on a 500-ledger window: 236,257 sig-matched → 43,504 (18%)
  survivors, so ~5× fewer topics reads (the quota-dominant cost the JOIN did NOT
  cut). `IN`-set membership also removes the unmerged-RMT JOIN fan-out. Window
  bumped 500 → 5,000 (memory now scales with survivors). Same-set verified
  (43,504 = 43,504).
- **asset_code parity spot-check (prod):** op-path stores short codes trimmed
  (`ETH` len 3, hex `455448`, no padding) = SEP-11 event code (`ETH`) → identical
  `asset_id`, no phantom duplicate rows. Closed.

Net LOC after cleanup: event_filters −180ish, stage `event_asset_id` gone. All
green: rustfmt --check, clippy, `cargo check --workspace`, full test suites.
