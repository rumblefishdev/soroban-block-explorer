---
title: 'Protocol 30: checks on production data'
type: research
status: mature
---

# Protocol 30: checks on production data

## Executor check

`backfill-runner contract-metadata-backfill --dry-run` on production
(mainnet, read-only user), 2026-10-19, built from `develop` (host 29, the
latest ledger's protocol 29) and from this branch (host 30, protocol 30):

| Build   | contracts | same as stored | different | failed | needs contract data |
| ------- | --------- | -------------- | --------- | ------ | ------------------- |
| host 29 | 4,552     | 4,378          | 0         | 115    | 59                  |
| host 30 | 4,552     | 4,378          | 0         | 115    | 59                  |

Every stored row was written by host 29, so host 30 reproduces all 4,378.
The tool logs no contract ids for `failed` / `needs contract data`; equal
counts, not proven equal sets.

## `M…` recipients lost their id (measured 2026-10-19)

Read-only, production mainnet, ledgers ≥ 64,500,000: 587,150 operations
carry a token event with a `u64` `to_muxed_id`. 31,025 already had the id
in `asset_transfers`; of the other 556,125, 556,112 are classic payments
whose transaction has an id memo (CAP-67 forwards it; `transaction_memos`
holds it). The remaining 13 have no id in `asset_transfers` and no id memo:
2 burns; 9 transfers from a claimable balance (`B…`, a classic claim, which
this change leaves as it was — CAP-67 adds `to_muxed_id` there only from a
memo, so why these carry one is unexplained); and 2 Soroban transfers to an
`M…` — from a contract (ledger 64,830,229) and from a `G…` (ledger
65,023,735) — whose id this change now keeps (owner: keep). Measured by
running `parse_ledger` + `build_value_flow_rows` on those archive ledgers
with `develop` and with this branch: exactly these 2 ids are new, every
other row is identical. Rows already written stay as they are until a
re-ingest of their range.

**Who emits a `u64` `to_muxed_id`** (production, ledgers ≥ 58,000,000,
2026-10-19): 2,458,236 events from SACs (44 deployed, 67 undeployed —
asset-shaped topics), 3 from one custom SEP-41 token (`CBJ4…24KZ`).
SEP-41 v0.4.0 defines `to_muxed_id` for every SEP-41 token, not only the
SAC.

## API check on production data (2026-10-19)

`api --bin local` on mainnet ClickHouse (read-only certificate), each
endpoint called with a base address and with its muxed form, status and
JSON body compared: accounts (detail, sponsorship, transactions), contracts
(detail, events, invocations, interface, verification, verified-source,
decompiled), assets (detail, transactions), liquidity pools (detail,
participants, activity, chart), NFTs (detail, transfers), the
`filter[source_account]` / `filter[contract_id]` list filters and search
(`M…`, `W…`, lower-case `w…`). 27 of 27 equal (`chart` minus its
now-relative `from`/`to`; `verified-source` an equal 404 for an unverified
pool). It found one defect, fixed in this PR: `chart` echoed the `W…` as
its `pool_id`. A broken-checksum `M…` is a 400.

Later the same day the owner dropped `W…` from `/assets/{id}` and
`/liquidity-pools/{id}` (no use for a muxed token or pool): those paths
answer a `W…` with 400 again, and the `chart` fix above went with them.
The owner then dropped `W…` from the NFT paths, the NFT filter and the
pasted `/nfts/W…/…` URL for the same reason; those answer as on `develop`.
