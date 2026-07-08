---
title: 'Schema + role mapping + before/after (design answers, in lieu of an ADR)'
type: generation
status: developing
spawned_from: notes/S-design-options.md
spawns: []
tags: ['schema', 'design', 'role-mapping']
links: []
history:
  - date: 2026-07-08
    status: developing
    who: karolkow
    note: >
      karolkow opted for NO separate ADR — design answers recorded here instead.
      Captures the before/after row shape, where roles and op-type come from, the
      old-vs-new table relationship, and the op-type → role mapping.
---

# Design answers (recorded here instead of an ADR)

## Where the `role` comes from

The role is **not stored anywhere today** — it is **assigned at parse time from
which named field of the operation the asset sits in.** Every op type has named
slots; each slot maps to a role:

- path-payment: `sendAsset` → `sent`, `destAsset` → `received`, `path[]` hops →
  `traded`;
- offer: `selling` → `sold`, `buying` → `bought`;
- change-trust: the asset → `trustline`;
- create/claim claimable balance: `escrowed` / `released`.

The parser reads the raw operation, sees which slot the asset occupies, and tags
the emitted row with the matching role. Role = "which slot it came from".

## Where the op `type` comes from (already correct)

The `type` column in `operations_appearances` (1 = payment, 3 = manage sell
offer, 13 = path-payment-strict-send, …). **Each operation declares its own type
in the raw XDR; the parser just reads it.** This is already stored correctly
(the diagnosis confirmed the type numbers read right) — it is NOT part of the bug.

## Row shape — before vs after

Example op: send 100 USDC → receiver gets 500 XLM, routed through EURT (3 assets).

**Now — `operations_appearances`, ONE row per operation:**

| column            | value                                                                 |
| ----------------- | --------------------------------------------------------------------- |
| ledger_sequence   | 58000000                                                              |
| transaction_id    | abc123…                                                               |
| application_order | 1                                                                     |
| type              | 13 (path-payment)                                                     |
| source_id         | GA…(sender)                                                           |
| asset_code        | **'' (empty!)** ← destination XLM landed here, native stored as empty |
| asset_issuer_id   | NULL                                                                  |
| amount            | …                                                                     |

One asset slot; USDC + EURT are dropped, XLM stored as empty (→ invisible).

**After — new `operation_asset_appearances`, one row per (op, asset, role):**

| asset_id               | role         | ledger   | tx     | app_order | amount |
| ---------------------- | ------------ | -------- | ------ | --------- | ------ |
| USDC                   | sent         | 58000000 | abc123 | 1         | 100    |
| EURT                   | traded (hop) | 58000000 | abc123 | 1         | —      |
| XLM (native surrogate) | received     | 58000000 | abc123 | 1         | 500    |

More examples:

- **Sell offer** "sell 100 XLM for USDC" — now: 1 row, asset **empty** (offers
  record no asset at all). After: 2 rows — `XLM/sold`, `USDC/bought`.
- **Plain payment** "50 USDC A→B" — now: 1 row, USDC (works). After: 1 row,
  `USDC/sent`. No change; single-asset ops are fine.
- **Claimable balance** "lock 200 AQUA for Bob" — now: create stores AQUA, claim
  stores only the balance-id (asset lost). After: create → `AQUA/escrowed`,
  claim → `AQUA/released` (asset recovered from the same-op `LedgerEntryChanges`).

## Old table does NOT disappear

The new table is a **companion index**, not a replacement:

- `operations_appearances` (old) stays — one row per operation; it still powers
  the tx-detail page, operation lists, etc.
- `operation_asset_appearances` (new) is "which asset appeared in which
  operation" — one row per (op, asset). The asset-page query switches to reading
  it; operation summary (hash/time/type) is still taken from the operation record.

This mirrors the existing `transaction_participants` ("which accounts were in
which transaction") — we add the analogous "which assets were in which operation".

## Complete op-type → role mapping (Phase 2 — all 25 Stellar op types)

`src` = declared/body role from the operation's named slots; `result` = added
per result-side `ClaimAtom` (crossed offers/pools) → always `traded`. "no asset
participation" = a recorded, deliberate N/A (not a silent drop).

| #   | Op                               | Assets                  | Role(s) → key                                                                                                                        |
| --- | -------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 0   | create_account                   | XLM starting balance    | funder → `sent`; new account → `received`                                                                                            |
| 1   | payment                          | 1                       | source → `sent`; dest → `received`                                                                                                   |
| 2   | path_payment_strict_receive      | 2–7                     | `sendAsset`→`sent`, `destAsset`→`received`, hops/atoms→`traded`                                                                      |
| 3   | manage_sell_offer                | 2                       | `selling`→`sold`, `buying`→`bought`; crossed→`traded`                                                                                |
| 4   | create_passive_sell_offer        | 2                       | `sold`, `bought`; crossed→`traded`                                                                                                   |
| 5   | set_options                      | 0                       | **N/A** (inflation_dest is a setting, not asset movement)                                                                            |
| 6   | change_trust (asset)             | 1                       | `trustline`                                                                                                                          |
| 6   | change_trust (PoolShare)         | 2 + pool                | `lp_a`, `lp_b` (+ pool id) — **3-entity keying, open sub-decision**                                                                  |
| 7   | allow_trust                      | 1 (third-party trustor) | `authorize` (admin; target = trustor, not signer)                                                                                    |
| 8   | account_merge                    | XLM                     | merged acct → `sent`; destination → `received`                                                                                       |
| 9   | inflation                        | XLM to dest             | **skip** (dead on mainnet; historical only — recorded decision, not silent)                                                          |
| 10  | manage_data                      | 0                       | **N/A**                                                                                                                              |
| 11  | bump_sequence                    | 0                       | **N/A**                                                                                                                              |
| 12  | manage_buy_offer                 | 2                       | `selling`→`sold`, `buying`→`bought`; crossed→`traded`                                                                                |
| 13  | path_payment_strict_send         | 2–7                     | `sendAsset`→`sent`, `destAsset`→`received`, hops/atoms→`traded`                                                                      |
| 14  | create_claimable_balance         | 1                       | `escrowed`                                                                                                                           |
| 15  | claim_claimable_balance          | 1 (**from meta**)       | `released` — asset NOT in body (only `balanceId`); resolve from same-op `LedgerEntryChanges`                                         |
| 16  | begin_sponsoring_future_reserves | 0 (XLM reserve)         | **N/A** (reserve mechanics, not asset participation — recorded)                                                                      |
| 17  | end_sponsoring_future_reserves   | 0                       | **N/A**                                                                                                                              |
| 18  | revoke_sponsorship               | 0 (target entry)        | **N/A** (reserve/ownership, not asset movement — recorded)                                                                           |
| 19  | clawback                         | 1                       | `clawed_back` (admin seizes from a holder; not lumped into a generic admin role)                                                     |
| 20  | clawback_claimable_balance       | 1 (**from meta**)       | `clawed_back` — asset via the removed `ClaimableBalanceEntry`                                                                        |
| 21  | set_trustline_flags              | 1 (third-party trustor) | `authorize` (admin; target = trustor)                                                                                                |
| 22  | liquidity_pool_deposit           | 2 + pool                | `lp_a`, `lp_b` (+ pool id)                                                                                                           |
| 23  | liquidity_pool_withdraw          | 2 + pool                | `lp_a`, `lp_b` (+ pool id)                                                                                                           |
| 24  | invoke_host_function             | variable                | **not in this table** — SAC/Soroban token flow via `soroban_invocations_appearances` + decoded `soroban_events` (union at read time) |

Every result-side `ClaimAtom` (order-book or LP crossing) emits a `traded`
participation for BOTH its `assetSold` and `assetBought` — this is the unbounded
grain and where path hops actually surface (see
[R-external-cross-validation](R-external-cross-validation.md), the devils-advocate
Concern 2 in [S-devils-advocate](S-devils-advocate.md): the trade fan-out MUST
emit both legs of every atom or thin-liquidity hops silently drop).

**Decisions resolved here (were flagged as gaps by the devils-advocate):**

- `clawback` / `clawback_claimable_balance` get their own `clawed_back` role.
- `allow_trust` / `set_trustline_flags` get `authorize`, keyed to the **trustor**
  (the affected third party), not the signer.
- `create_account` / `account_merge` index their XLM movement (`sent`/`received`).
- `inflation` → **skip** (dead); sponsorship 16/17/18 → **N/A** (reserve
  mechanics). Both are recorded decisions, not silent omissions.
- `claim_claimable_balance` / `clawback_claimable_balance` asset comes from the
  same-op `LedgerEntryChanges` (the removed `ClaimableBalanceEntry`) — cheaper
  than a cross-partition CB-id join, and the same gap our own advanced tx-view
  hits today (it shows only `balanceId`).

## Resolved — PoolShare / LP keying (decision a, 2026-07-08)

It is **not** a 3-way asset puzzle. Pools are already a first-class dimension
(`pool_ids` array + the `liquidity_pools` table + pool detail pages), so the pool
never competes for an `asset_id` slot.

- **LP deposit / withdraw (22/23):** emit **two** asset rows — `A → lp_a`,
  `B → lp_b` — each carrying a `pool_id` column linking to the existing pool
  dimension. The minted/burned pool-share and the pool itself are covered by the
  pool page, not a third asset row.
- **change_trust PoolShare (6):** a trustline to the pool _share_, not a movement
  of A or B → a **pool-dimension** event (`trustline` on the `pool_id`), NOT asset
  rows for A/B (a trustline to the pool is not activity of asset A). Keep it off
  the asset fan-out; it shows on the pool page.

So the fan-out stays strictly per-(op, asset, role) with a `pool_id` link column;
the "3 entities" dissolve into 2 assets + the already-indexed pool. No open keying
question remains.

## `leg_index` — content-addressed determinism (Critical gate b)

`leg_index` is the last field of the RMT sort key
`(asset_id, ledger_sequence, transaction_id, application_order, role, leg_index)`.
Its only job: disambiguate multiple participations that share (asset_id, role)
within one op (e.g. an asset that appears in two crossed atoms as `traded`). It
MUST be **stable** (byte-identical between live ingest and S3 backfill re-parse)
and **dense/unique** within (op, asset_id, role) — else RMT double-counts
(backfill rows don't collapse) or under-counts (distinct legs collapse), invisible
until diffed vs Horizon.

**Derivation — a fixed, XDR-order enumeration (NOT iteration/assembly order):**

1. Enumerate an operation's (asset, role) participations in a FIXED order that is
   a pure function of the parsed XDR:
   - declared legs in a fixed slot order — `sendAsset`(0), `destAsset`(1),
     `path[0]`(2), `path[1]`(3), … then `selling`/`buying`/`asset`/… per op kind;
   - then result `ClaimAtom`s in their `offers<>` **vector order**, each atom
     contributing `assetSold` then `assetBought`.
2. `leg_index` = the running ordinal WITHIN each (asset_id, role) as they appear
   in that enumeration (0, 1, 2, …).

The enumeration is driven by XDR slot/vector positions — never a `HashMap` /
random / insertion order — so it is byte-identical between the live parser and the
backfill crate parsing the same archived XDR.

**Hard requirements (the gate — Phase 2 does not ship without these):**

- **One shared library function** `emit_asset_participations(op_details,
op_result) -> Vec<Participation>`, called by BOTH live ingest and the backfill
  crate. NEVER duplicate the enumeration in the backfill crate (the divergence the
  devils-advocate flagged — [[feedback_backfill_new_crate]] says backfill = a new
  crate, so the shared function must live in a common lib both depend on).
- **No non-deterministic iteration** in that function for ordering (mirror the
  `pool_ids` sort-determinism discipline `stage.rs:934-935`, but for an ORDINAL,
  not a set).
- **Differential test (blocking):** run a fixture set (one op per type +
  multi-atom path payments + multi-op txs) through BOTH the live and backfill
  paths; assert byte-identical fan-out rows (asset_id, role, leg_index, amount…).
- **Parser-version guard:** any change to the enumeration (new leg kinds,
  reordered slots) shifts `leg_index` → re-keys history; freeze it, gate changes
  behind the differential test + a re-backfill.

**Alternative (recorded, not chosen):** a content hash `leg_index = hash(grain,
source_index, assetSold, assetBought, amountSold, amountBought)` — collision-
resistant but not dense/sortable and heavier. The fixed ordinal is already
deterministic (same XDR → same order), so prefer it; keep the hash only if a
future leg source is genuinely order-unstable. See
[S-devils-advocate](S-devils-advocate.md) Critical gate 2.

## External parity (why this shape)

Mature explorers show per-asset detail per operation, which we don't:
stellar.expert renders "sent 49.65 USDC", "swapped X → USDC", path hops, and a
Trades tab; Hubble stores TWO endpoint assets (source + dest) as columns. We keep
only one asset when there are several — the odd one out. Details:
[R-external-cross-validation](R-external-cross-validation.md),
[S-field-comparison-fat-thin](S-field-comparison-fat-thin.md).
