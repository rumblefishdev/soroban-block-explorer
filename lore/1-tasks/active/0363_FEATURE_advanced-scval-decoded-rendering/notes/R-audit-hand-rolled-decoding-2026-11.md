---
title: 'Audit: hand-rolled decoding of Stellar data (2026-11-17)'
type: research
status: developing
spawns: []
tags: [xdr, data-fidelity, frontend, api]
links: []
history:
  - date: '2026-11-17'
    status: developing
    who: karolkow
    note: 'Read-only audit of develop @ 52c1ea088; each finding routed to the open task that owns its area.'
---

# Audit: hand-rolled decoding of Stellar data (2026-11-17)

Read-only audit of `develop` @ 52c1ea088: places where our code decodes,
encodes or formats Stellar data itself instead of calling `stellar-xdr` 30 (or
the frontend's own helpers), and where that loses or distorts a value.
"Verified" means re-checked on develop by a second reader; the rest are the
auditor's claims, to verify when the owning task picks them up.

Cause shared by 1 and 3: the generated API client parses with plain
`JSON.parse` (`libs/api-types/src/generated/client/client.gen.ts`), which rounds
integers above 2^53.

| #   | Finding                                                                                                                                                                                                                                                             | Evidence                                                                        | Verified  | Owner                              |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | --------- | ---------------------------------- |
| 1   | Account page shows a rounded sequence number: `AccountDetailResponse.sequence_number: i64` (`crates/api/src/accounts/dto.rs:105`) reaches `formatAmount` (`AccountSummary.tsx:77`) as a JSON number                                                                 | 17,545,046 of 20,260,212 accounts (86.6%) above 2^53 (`chq`, 2026-11-17)        | yes       | 0363, with the value-format change |
| 2   | 64-bit values in operation details and ScVal (`bumpTo`, `offerId`, `limit`, claim atoms, `ledger_key_nonce`) render rounded in the raw-JSON view, trace and sentences                                                                                               | mechanism: `JSON.parse("18446744073709551615")` → `18446744073709552000`        | mechanism | 0363, with the value-format change |
| 3   | Change-trust to a pool share keeps only `params.name()` (`crates/xdr-parser/src/operation.rs:658`): assets and fee lost                                                                                                                                             | official `stellar xdr decode --type ChangeTrustAsset` keeps both assets and fee | yes       | 0456                               |
| 4   | Revoke-sponsorship of a ledger entry keeps only `key.name()` (`operation.rs:519`): which entry is lost                                                                                                                                                              | `LedgerKey` official JSON carries the whole key                                 | yes       | 0456                               |
| 5   | Create-contract drops `contract_id_preimage` (deployer + salt, or the wrapped asset); upload-WASM keeps only its length                                                                                                                                             | reading of `extract_invoke_host_function`                                       | no        | 0456                               |
| 6   | Liquidity-pool and claimable-balance ids are bare hex (`operation.rs:275,552,565,809`, `ledger_entry_changes.rs`); the library and SEP-23 use `L…` / `B…` (`PoolId`, `ClaimableBalanceId` `Display`). One card shows the same pool as hex and as `L…`               | official CLI: `--type PoolId` → `LCV2…`                                         | yes       | 0456                               |
| 7   | NFT token id loses its ScVal type (`state/nfts.rs` `token_id_to_string`); `token_uri` is then always called with `ScVal::U32` (`crates/enrichment-shared/src/nft_token_uri/client.rs:318,489`), so a u64/i128-id contract never gets its URI                        | reading                                                                         | yes       | 0392                               |
| 8   | Exact `CODE:ISSUER` search folds case (`crates/api/src/search/queries/assets.rs:131`) although codes are case-sensitive per issuer                                                                                                                                  | 5,421 (issuer, lower(code)) groups with several case variants                   | yes       | none open (0443 is archived)       |
| 9   | Offer prices and pool rates below 5e-8 render as `0` (`humanizeOp.ts` `priceStr`: `maximumFractionDigits: 7`; `PoolActivity.tsx`)                                                                                                                                   | `(1/30000000).toLocaleString('en-US',{maximumFractionDigits:7})` → `"0"`        | yes       | 0363                               |
| 10  | A sentence keys full addresses by their 4…4 form (`HumanizedSentence.tsx`): two lookalike addresses in one operation link to one of them                                                                                                                            | reasoning only                                                                  | no        | 0363, prove first                  |
| 11  | Route strip matches hops by code only (`USDC:A → USDC:B` reads `USDC → USDC`); call tree shows `createContractV2(0)` because `args` is an object (`CallTree.tsx`); SET_TRUST_LINE_FLAGS downgrade reads "Revoked authorization"; muxed `M…` sub-account never shown | reading                                                                         | partly    | 0363                               |
| 12  | Three policies for invalid UTF-8 in one pipeline (memo hex, `<invalid-utf8>` placeholder, U+FFFD); `data_value` base64 in the operation but hex in the entry change                                                                                                 | 0 U+FFFD in `accounts.home_domain` (20.27 M)                                    | no        | 0431                               |
| 13  | `format_stroops` drops the sign between −1 and −9,999,999 stroops (`state.rs`); only non-negative balances use it today                                                                                                                                             | reading                                                                         | no        | 0431                               |
| 14  | `decimal7_string_to_i128` reads `"5"` as 5 but `"5.0"` as 50,000,000 (`persist/stage.rs`); callers consistent today                                                                                                                                                 | reading                                                                         | no        | 0431                               |
| 15  | `asset_code_str` renders invalid UTF-8 as `0x…`, indistinguishable from 11 real codes starting with `0x`                                                                                                                                                            | 0 non-alphanumeric codes in `assets`                                            | no        | 0431                               |
| 16  | Contract-spec reader stops silently at the first undecodable entry (`contract.rs` `parse_spec_entries`, `Err(_) => break`)                                                                                                                                          | reading                                                                         | no        | 0431                               |
| 17  | Duplicates of library functions with no defect: `encode_xdr`, `pool_id_hex_to_strkey`, `build_credit_asset`, frontend SEP-23 strkey                                                                                                                                 | reading                                                                         | no        | 0431                               |

Not covered: pool factory/router/state readers, the invocation auth tree,
`net_settled.rs`, `op_participants.rs`, backfill snapshot readers, the archive
merge path in the API, USD float math.

## Second pass (2026-11-18): official stellar-xdr JSON vs ours, and areas not covered above

Read-only, `develop` @ 7729b5edd; library sides read in stellar-xdr 30.0.0.
Only the `std` and `alloc` features are on; the official JSON (SEP-51) needs
the `serde` feature.

Our contract-value JSON (`crates/xdr-parser/src/scval.rs`) against the
official one:

| Value                         | Ours                               | Official (SEP-51)                      |
| ----------------------------- | ---------------------------------- | -------------------------------------- |
| envelope                      | `{"type":"u32","value":5}`         | `{"u32":5}`                            |
| u64 / i64 / timepoint / nonce | JSON number                        | decimal string                         |
| u256 / i256                   | 64 hex characters, no sign         | signed decimal                         |
| bytes                         | base64                             | hex                                    |
| string / symbol               | text, `<invalid-utf8>` placeholder | escaped bytes (lossless); tag `symbol` |
| error                         | variant name only                  | type and code (`{"contract":7}`)       |
| map entry                     | `{key, value}`                     | `{key, val}`                           |

New findings (verdict and owner):

| #   | Where                                                                                            | What                                                                                                                             | Loses data?     | Verdict, owner  |
| --- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- | --------------- | --------------- |
| 18  | `scval.rs:19`                                                                                    | a contract error keeps its name only; the code (`#7`) is lost                                                                    | yes             | adopt, 0363     |
| 19  | `invocation.rs:431-527`                                                                          | the auth tree keeps only the invocation; credentials (address, nonce, expiry) and the create-contract preimage are lost          | yes             | adopt, 0456     |
| 20  | `ledger_entry_changes.rs:226,273,339-364`                                                        | config-setting entries stored as `{}`; account and trustline entries drop `inflation_dest`, liabilities, `seq_ledger`/`seq_time` | yes             | no open task    |
| 21  | `rpc_snapshot.rs:410,417`, `network_state.rs:449`                                                | asset code through `from_utf8_lossy`, not the canonical reader                                                                   | latent          | adopt, 0431     |
| 22  | `persist/stage.rs:2068` (8 calls)                                                                | an account strkey checked by length and first letter; the library parses it                                                      | accepts junk    | adopt, 0431     |
| 23  | `contract.rs:56-153,261`                                                                         | own WASM section parser and XDR stream loop; the library has `read_xdr_iter`                                                     | no (duplicate)  | adopt, 0431     |
| 24  | `transaction.rs:196-213`                                                                         | `encode_xdr` returns `""` on error                                                                                               | hides a failure | adopt, 0431     |
| 25  | i128 from parts, muxed split, strkey → ScAddress (5 sites), contract executable rendered 3 times | duplicates of library functions                                                                                                  | no              | adopt, 0431     |
| 26  | `libs/ui/src/identifiers/strkey.ts`                                                              | own strkey encoder in the web; no `@stellar/*` package is installed                                                              | no              | decision needed |
| 27  | `usd_analytics.rs:397`, `get_pool_chart.rs:392`                                                  | `raw as f64 / 10^d`; an exact scaler exists (`soroban_reserves.rs:120`)                                                          | above 2^53 raw  | no open task    |

Kept with a reason: pool, factory, asset-transfer and executable readers
pattern-match our JSON because they read the persisted dialect (a DB
contract); shape-only strkey checks in the API (ADR 0037); the
`CODE:ISSUER` asset string (no library `Display` for `Asset`); TOID math (no
library equivalent).

Not covered: web call tree and raw-JSON internals, API search SQL, the
contract-executor crate, SEP-1 and NFT URI enrichment, snapshot claimable /
pools / entry-state readers, `classification.rs`.
