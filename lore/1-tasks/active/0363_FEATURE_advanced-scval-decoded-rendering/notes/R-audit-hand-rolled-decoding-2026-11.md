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
