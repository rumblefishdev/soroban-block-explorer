---
id: '0349'
title: 'BUG: account "deleted" detection wrong — redesign to last-lifecycle (merge vs create), not last_seen anchor'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0324', '0321', '0295']
tags:
  [
    clickhouse,
    accountmerge,
    accounts,
    api,
    correctness,
    priority-high,
    effort-medium,
  ]
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: karolkow
    note: >
      Found via two live witness accounts whose "Deleted" badge never renders
      though they are merged (Horizon 404). Root-caused to 3 independent flaws
      in the 0324 detection query, verified against stellar-core XDR + live
      mainnet + prod ClickHouse (devil's-advocate pass). Fix is a query
      REDESIGN, not a patch. Analysis-only; no code changed.
---

# BUG: account "deleted" detection is wrong — redesign to last-lifecycle rule

## Summary

The `deleted` flag from task 0324 (`fetch_deleted_status`) misses most merged
accounts. Three independent flaws, each proven on live mainnet + prod CH. The
correct rule is a **last-lifecycle** comparison (last successful `account_merge`
as effective source vs last successful `create_account` as destination), not the
current single-ledger `last_seen`-anchored `EXISTS` on `op.source_id`. This is a
query redesign in `crates/api/src/accounts/queries_ch.rs`.

## Context — witnesses

Both render NO "Deleted" badge in the explorer but are merged out on mainnet
(should be badged like stellar.expert):

- `GBRTYCLZUKSACCL7S7P3THNVUXJM2FZ4XT5LO3GUSVM64GX5DVFHMHTX` — merged at ledger
  50,457,435. Fails via **flaw 1**.
- `GDGDR2IIGZZ6MERAY7VH4REOMBZK2H7EZKWUGVTDYNHJ6EZQZUCJAFSC` — merged at ledger
  62,016,086; a later **failed** payment (dest = this account) at 62,016,089
  bumped `last_seen_ledger` past the merge. Fails via **flaw 2**.

## The three flaws (all in `fetch_deleted_status`, current query)

Current query (0324): `EXISTS` a `type=8` op with `oa.source_id = <account>` in
a **successful** tx, anchored to `accounts.last_seen_ledger`.

1. **`op.source_id` is NULL for own-account merges.** Per XDR, an operation with
   no explicit source inherits the **transaction** source. You merge your _own_
   account → the merge op carries no separate source → `source_id = NULL` → the
   `= <account>` match fails. **Measured: 4,865,519 of 6,302,100 merge ops
   (~77%) have NULL `source_id`.** 0324 only "passed" because its one test
   account was the atypical explicit-op-source case.
   **Fix:** match on `coalesce(op.source_id, tx.source_id)`.

2. **Anchoring on `last_seen_ledger` is wrong.** `last_seen_ledger =
GREATEST(all appearances)` includes **failed txs and incoming references**
   (e.g. a failed payment _to_ the deleted account) that land in a _later_
   ledger, pushing the anchor past the merge ledger → the merge is not in the
   anchored ledger → `deleted = false`.
   **Fix:** do not anchor on `last_seen_ledger` at all.

3. **Re-creation / recycling not handled (biggest hole).** A merged account
   re-created via `create_account` is **alive**, but `create_account` is sourced
   by the **funder**, not the account — so a "last self-sourced op" heuristic
   still sees the old merge and wrongly reports deleted. **Measured: up to ~47%
   of a window's successful-merge accounts are recycled and currently alive**
   (found a keypair created+merged ~12×). Must compare merge vs a later
   `create_account`.

## The correct rule (100%-defensible, verified vs stellar-core + mainnet)

> An account is `deleted` **⟺** it has a **successful `account_merge`** with
> effective source = A, **AND no successful `create_account`** targets A at a
> strictly later `(ledger_sequence, application_order)`.
>
> `deleted = max(ledger,order | type=8, successful, effective_source = A)` > `        > max(ledger,order | type=0, successful, destination = A)`
> (deleted = false if no successful merge exists; A alive if a later create wins.)

Where **effective source** = `coalesce(op.source, INNER_tx.source)`, normalized
to the bare **ed25519 G-key** (strip any muxed M-address). Never the fee-bump
`feeSource`. Never `last_seen_ledger`.

### Why each clause (protocol-grounded — stellar-core `MergeOpFrame.cpp`, XDR, CAPs)

- **`account_merge` is the ONLY AccountEntry deletion path** (no zero-balance
  reaping, no Soroban archival of accounts, no sponsorship deletion) — so merge
  is the sole deletion signal.
- **successful only** — a failed merge (`HAS_SUB_ENTRIES`, `IS_SPONSOR`,
  `DEST_FULL`, `SEQNUM_TOO_FAR`, …) does NOT delete; needs the `transactions`
  join for `t.successful`.
- **coalesce + G-normalize** — op-source inheritance (flaw 1) + muxed sources
  resolve to the same G-key AccountEntry (which is never muxed).
- **fee-bump inner source** — for fee-bumped merges, the op inherits the _inner_
  tx source, not `feeSource`. Verify our `transactions.source_id` is the inner
  source for fee-bump rows (`inner_tx_hash IS NOT NULL`).
- **merge vs create** — handles recycling (flaw 3).

## Implementation Plan

### Step 1 — rewrite `fetch_deleted_status` (CH)

Replace the `last_seen`-anchored `EXISTS` with the two-argMax comparison above.
Drop the `last_seen_ledger` parameter. Read-cost: the merge/create lookups scan
by account without a single-ledger partition prune — smoke-test read/memory on
prod (the 3.73 GiB api_reader cap is real; the analysis queries OOM'd on naive
joins). Consider deriving `effective_source` once during ingestion into an
account-keyed column/table if the read cost is too high.

### Step 2 — muxed + fee-bump correctness

Confirm ingestion stores the bare G-key for op/tx source (not raw M-address),
and the inner-tx source for fee-bump. Add normalization if not.

### Step 3 — regression fixtures

- `GBRTY…MHTX` → `deleted = true` (flaw 1).
- `GDGDR2…AFSC` → `deleted = true` (flaw 2).
- `GC3UIDADRXRQROZWCDZK2BOSYSE4FUNHRDGWKLIHNSX3AJ3WPXW323PI` → `deleted = false`
  (recycled/alive; flaw 3 — created+merged ~12×, currently live).

## Acceptance Criteria

- [ ] `deleted = true` for both merged witnesses (`GBRTY…`, `GDGDR2…`)
- [ ] `deleted = false` for the recycled-alive witness (`GC3UIDAD…`)
- [ ] Detection independent of `last_seen_ledger`
- [ ] `coalesce(op.source, tx.source)`, G-normalized, successful-only
- [ ] Cross-checked against live Horizon 404/200 on a sample of ≥20 accounts
- [ ] Prod read/memory smoke on the new query (no api_reader OOM)
- [ ] **Docs updated** — update the `11_get_contracts_by_id.sql`-adjacent
      accounts query doc / `docs/architecture/**` if the deleted-derivation
      shape is documented there.
- [ ] **API types regenerated** — N/A unless the DTO changes (the bare
      `deleted: bool` surface from 0324 stays).

## Verified example accounts (extended fixtures + evidence)

Sampled from prod ClickHouse and cross-checked against **live mainnet Horizon**
(`GET /accounts/{id}` → 404 = deleted, 200 = live) on 2026-07-03. Use as
regression fixtures beyond the 3 primary witnesses.

**Flaw 1 — own-account merge, `op.source_id` NULL, `last_seen == merge`**
(Horizon: all 404 = deleted; explorer currently shows them live):

| Account                                                    | Merge ledger | Native ghost XLM | Horizon |
| ---------------------------------------------------------- | ------------ | ---------------- | ------- |
| `GBRTYCLZUKSACCL7S7P3THNVUXJM2FZ4XT5LO3GUSVM64GX5DVFHMHTX` | 50,457,435   | ~5               | 404     |
| `GDG6Z2T7DM5L7R2G7U3LPKF6N5PIJ5C2FFUR3PYXENWFW676TUOEPBH7` | 50,457,439   | 5                | 404     |
| `GB5NO77A5Z3CW5K37NESPAWDVDHSANTUKGIUPWVIRI33YFO4VVVSLKSZ` | 50,457,457   | 5                | 404     |

**Flaw 2 — merged, then a later op (failed/incoming, foreign source) bumped
`last_seen_ledger` past the merge** (Horizon: all 404):

| Account                                                    | Merge ledger | last_seen  | gap | Horizon |
| ---------------------------------------------------------- | ------------ | ---------- | --- | ------- |
| `GDGDR2IIGZZ6MERAY7VH4REOMBZK2H7EZKWUGVTDYNHJ6EZQZUCJAFSC` | 62,016,086   | 62,016,089 | +3  | 404     |
| `GBNVSS2GEU3U633JFLRQHFCAZGQVB2BZX4PO3RBTRC6LKNWSTLMI6D6H` | 61,000,423   | 61,000,426 | +3  | 404     |
| `GCB5IJJBVIC27L3FYYFYTPYDFMXKYH2CBIZTZQ5WLYXVN6MELJIRISUY` | 61,024,239   | 61,024,242 | +3  | 404     |
| `GBOEFOXJ52ZSPPCKG4MRQA56VNV4QBUXGYAWFFRB5BRD7Q5BTPLB6XMK` | 61,010,558   | 61,010,564 | +6  | 404     |

**Flaw 3 — recycled (create→merge repeated hundreds/thousands of times);
oscillates alive/dead — must NOT be permanently flagged deleted:**

| Account                                                    | Times created (100k-ledger window) | Note                          |
| ---------------------------------------------------------- | ---------------------------------- | ----------------------------- |
| `GC3UIDADRXRQROZWCDZK2BOSYSE4FUNHRDGWKLIHNSX3AJ3WPXW323PI` | ~12×                               | held token (SSLX) while alive |
| `GABFJIMI52DCECD7ZSMNEFCNEYKREJOKPQCM2QX5UKCVY3UHHA77JNL6` | 3,822×                             | arb-bot ephemeral keypair     |
| `GCDBGSAJKINKZCSKJKVRCXIKE4XCZGXJWSQTHMFFL5XXJTTY3MUHW7VS` | 1,646×                             | "                             |
| `GDT7V37D57LPZD3SFHDL3OMP62ZODNOO3MBJX6KZDXMYBBNITRE2MTIH` | 1,547×                             | "                             |

Note on flaw 3: at any instant a recycled account may be 404 (merged phase) or
200 (alive phase). The correct rule (last merge vs last create) must return the
**current-phase** answer — `deleted=true` only when the last lifecycle event is
the merge. A token balance seen on such an account in our DB is a **stale** row
from a prior alive phase, not current holdings (truly-deleted accounts never
hold tokens — trustlines must be closed before merge).

## Notes

- Frontend already renders the badge on `deleted === true`
  (`web/src/pages/AccountDetailPage.tsx:74`) — no FE change needed; this is a
  backend correctness fix.
- Sibling task **0321** fixes the stale _balance_ on deleted accounts (native
  tombstone backfill) — orthogonal but same population; see cross-refs.
- Verified: deleted accounts can NEVER hold a non-native balance (merge requires
  all trustlines closed, `HAS_SUB_ENTRIES`) — so no balance-related edge case
  affects this detection.
