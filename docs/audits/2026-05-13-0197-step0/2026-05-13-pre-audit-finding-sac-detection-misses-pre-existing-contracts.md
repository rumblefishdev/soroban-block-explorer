# Pre-audit finding: `soroban_contracts.is_sac` not flagged for pre-existing SAC wrappers

**Date:** 2026-05-13
**Status:** open
**Source:** Step 0 mini-spike of task 0197 (DB completeness audit), cross-check with stellar.expert
**Severity:** medium — same architectural pattern as findings #1 / #2 (indexer state-driven path fails on pre-existing entities).

## TL;DR

When the indexer sees a Soroban contract for the first time (e.g. as
the emitter of an event), it inserts a row into `soroban_contracts`
with `is_sac = false` by default. The flag is set to `true` only if
the indexer **observes the SAC deployment operation** within the
indexed ledger window. For SAC wrappers deployed years ago (XLM, USDC,
yUSDC, yBTC), that deployment is far outside any realistic backfill
window — so `is_sac` stays `false` even though the contract is in fact
a SAC.

## Repro

Local backfill of pubnet ledgers `50944000..50955110` (11k ledgers,
~10% of one 64k-ledger partition).

Top "NFT" contracts in our DB (by row count in `nfts`), all confirmed
SAC by stellar.expert API:

| `soroban_contracts.contract_id` | `is_sac` (ours) | stellar.expert says    |
| ------------------------------- | --------------- | ---------------------- |
| `CAS3J7GY…`                     | **false**       | SAC for `XLM`          |
| `CCW67TSZ…`                     | **false**       | SAC for `USDC-GA5ZSE…` |
| `CB2XMFB6…`                     | **false**       | SAC for `yBTC-GBUVRN…` |
| `CDOFW7HN…`                     | **false**       | SAC for `yUSDC-GDGTV…` |

```bash
$ curl https://api.stellar.expert/explorer/public/contract/CAS3J7GY...
{"contract":"CAS3J7GY…","asset":"XLM","creator":"GDMTVHL…","created":1708482496,…}

$ PGPASSWORD=postgres psql -h localhost -p 5434 -U postgres \
    -d soroban_block_explorer \
    -c "SELECT is_sac FROM soroban_contracts WHERE contract_id='CAS3J7GY…';"
 is_sac
--------
 f
```

All four contracts were `created` in early 2024 (UNIX `1708482496` ≈
2024-02-21). Our backfill window covers pubnet ledgers
`50944000..50955110` which closes ~2026; the SAC deployment ops
predate the window by years.

## Root cause

Same shape as the classic-credit-asset-row finding (file
`2026-05-13-pre-audit-finding-classic-credit-asset-row-missing.md`)
and the home-domain finding (file
`2026-05-13-pre-audit-finding-home-domain-backfill-gap.md`):

The indexer is purely **event-driven** — every column on every
"entity" table is derived from operations / events / ledger-entry
changes observed inside the indexed window. When an entity first
appears via a side-effect (e.g. SAC wrapper used as event emitter), a
row gets inserted with only the side-effect-derivable fields. Fields
that come from the **original creation event** (the SAC deployment,
the account's `SetOptions`, the classic-credit trustline establishment)
are NULL/false until that creation event is re-observed — which never
happens for entities older than the window.

For SAC specifically, `is_sac=true` requires observation of a
deployment with `ContractIdPreimage::FromAsset`. The indexer hits
contract `CAS3J7GY…` countless times as event emitter on transfers, but
never re-observes its 2024 deployment, so `is_sac` stays `false`.

## Impact

- `is_sac` is the central marker for "this Soroban contract is a SAC
  wrapper, not a custom contract". Many downstream paths key off it.
- ADR 0043 §Decision treats SAC contracts as equivalent to classic
  credits for the field-allocation rule (off-chain → Lambda 2). If
  `is_sac` is wrong, the rule is being applied to the wrong contracts.
- The NFT classifier in `crates/xdr-parser/src/classification.rs` would
  correctly route SAC contracts away from the NFT path _if_ it had a
  reliable `is_sac` signal. Today it relies on the half-broken
  `looks_like_token_id` heuristic instead (see the false-positives
  finding).
- Production state: any historical / well-known SAC wrapper imported
  by the live indexer will silently carry `is_sac=false`.

## Proposed fix

Bundle with finding #2 (home_domain backfill gap) since both share the
same root cause and likely share the same fix path:

When the indexer encounters a contract it has not seen before, perform
an initial-state RPC fetch:

```text
getLedgerEntry { key: LedgerKey::ContractData(contract_id, ScVal::LedgerKeyContractInstance) }
```

The response carries the deployment preimage. If the preimage is
`ContractIdPreimage::FromAsset(asset)`, set `is_sac=true` + populate
`(asset_code, issuer_id)` if classic credit (or NULL/NULL for native
XLM). Otherwise `is_sac=false` and the contract is custom WASM.

Cost: one RPC call per never-before-seen contract, cached afterwards.

The same RPC pathway is required for finding #2 (account backfill) and
finding #1 (classic-credit asset row creation), so a single
"indexer initial-state RPC enrichment" task could fix all three.

## Audit context

Finding #4 in 0197 Step 0 mini-spike. Together with #1 (assets row
missing), #2 (home_domain empty), and #3 (NFT false positives), it
forms a coherent pattern: **the indexer needs an initial-state load
path** for entities that pre-date the backfill window. Without it,
half the entity columns are wrong / missing / misclassified on a
realistic local backfill.

Tasks #1, #2, and #4 can all be fixed by the same upstream change
(initial-state RPC enrichment on first observation). Task #3 needs a
separate fix (replace `looks_like_token_id` with WASM classifier).
