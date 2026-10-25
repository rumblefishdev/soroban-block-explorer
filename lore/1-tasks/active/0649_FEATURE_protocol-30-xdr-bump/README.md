---
id: '0649'
title: 'Protocol 30: bump stellar-xdr 28 → 30 and soroban-env-host 29 → 30 before the vote'
type: FEATURE
status: active
related_adr: ['0026', '0061']
related_tasks: ['0646', '0605', '0548', '0620']
tags: [xdr, ingestion, protocol-30, priority-high, effort-small]
links:
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0084.md
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0087.md
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0088.md
  - https://github.com/stellar/stellar-horizon/pull/237
  - https://crates.io/crates/stellar-xdr/30.0.0
  - https://crates.io/crates/soroban-env-host/30.0.0
history:
  - date: '2026-10-19'
    status: active
    who: karolkow
    note: >
      Created after the 0646 watch logic reported LAGGING against live data:
      core 30.0.0 supports protocol 30, mainnet and testnet run 29, and
      soroban-env-host 30.0.0 on stellar-xdr 30.0.0 is on crates.io.
---

# Protocol 30: stellar-xdr and soroban-env-host bump

## Summary

Protocol 30 changes the XDR (CAP-0084, CAP-0087, CAP-0088). CAP-0088 makes the
new millisecond `StellarValueExt` arms mandatory from the first ledger after
the upgrade, so a parser on `stellar-xdr` 28 cannot decode any protocol-30
ledger: ingestion stops at the vote, on testnet first, then on mainnet. This
task moves the workspace to `stellar-xdr` 30 and makes the contract executor
work on both sides of the vote.

## Stan teraz

- Done: facts gathered (below); trial bump compiles with one match arm added;
  800 real ledgers parse identically on 28 and 30; clippy green.
- Found: `soroban-env-host` 30 runs only protocol-30 ledgers, so a plain bump
  breaks token metadata until the vote.
- Done: every protocol-30 change checked against our code
  ([notes/R-protocol-30-impact.md](notes/R-protocol-30-impact.md)).
- Done: executor on host 30 measured on production (below).
- Done: PR 1 (#673, bump + executor) merged 2026-10-19; CI green.
- Done: PR 2 branch `feat/0649-muxed-contract-addresses` (`W…` support,
  Soroban recipient id, #673 review fixes); not opened, the PR queue is full.
- Done: #674 merged. #681 (`[refactor]`) merged 2026-10-19: muxed
  resolution moved into `xdr-parser` (`asset_transfers/muxed.rs`);
  `asset_transfers` rows identical to `develop` on 349 mainnet ledgers.
- Next: deploy to testnet before its vote; then the `asset_transfers`
  backfill.
- Evidence: [notes/R-protocol-30-facts.md](notes/R-protocol-30-facts.md),
  [notes/R-protocol-30-checks.md](notes/R-protocol-30-checks.md) (executor,
  `M…` measurement, API on production data).

## Decisions so far

| Situation                                                                                                                  | What happens                                                                                                                                                                                                                              | Date       |
| -------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| A `W…` (muxed contract) address reaches the parser, DB or API                                                              | Full support: treated as its `C…` contract plus the muxed id wherever `M…` is treated as `G…` plus id; API path, filters and search accept `W…`; the UI links it to the contract page; `stellar-strkey` moves to 1.0                      | 2026-10-19 |
| stellar-prices-api builds on our `xdr-parser` from `develop`                                                               | Merge after review, no waiting; its team is told before the merge that its next `xdr-parser` update must move it to `stellar-xdr` 30                                                                                                      | 2026-10-19 |
| The executor runs a contract function for a ledger of any protocol (live before or after a vote, or a backfill of history) | One `soroban-env-host` (30); every run states the host's own protocol, since the host refuses any other so as never to replay a contract with another protocol's execution costs, which a read-only call is not charged for               | 2026-10-19 |
| A Soroban transfer names the recipient's sub-account in its event (`G…`/`C…` topic, `u64` `to_muxed_id` in data)           | `asset_transfers.to_muxed_id` takes it for a Soroban call; a classic operation keeps the envelope, since its `u64` may be an id memo (CAP-67 rule 2). Covers `W…` and `M…` alike: the owner kept the `M…` case (same rule, CAP-67)        | 2026-10-19 |
| A Soroban transfer's `to_muxed_id` comes from any token contract, not only a SAC                                           | Taken as the token emitted it: SEP-41 v0.4.0 defines it for every token, and a token speaks only for its own transfers (asset = emitter); documented, not filtered                                                                        | 2026-10-19 |
| An `M…` reaches the API or the UI                                                                                          | Same as `W…`: the account paths, `filter[source_account]` and search answer it as its `G…`; the UI links it to its account; pasted `/accounts/M…` and `/contracts/W…` URLs redirect to the base page; the transaction filters accept both | 2026-10-19 |
| A `W…` in `/assets/{id}`, `/liquidity-pools/{id}`, the `/nfts` paths and filter, or a pasted `/nfts/W…/…` URL              | Not accepted (400 as before): a token, a pool or an NFT collection has no sub-account use; `W…` stays where a muxed address really appears — contract and account pages, filters, search, UI links                                        | 2026-10-19 |
| A muxed address could show only its sub-account's data                                                                     | Not in this task: `M…`/`W…` answer as the base address; a sub-account view is task 0651                                                                                                                                                   | 2026-10-19 |
| The UI decodes `M…`/`W…` with its own ~60 lines (`libs/ui` `strkey.ts`)                                                    | Kept until `@stellar/stellar-base` releases `W…` support (js-stellar-sdk#1811, open on 2026-10-19); then replace it with the library                                                                                                      | 2026-10-19 |
| 171 historic Soroban transfers to an `M…` (ledgers 58,781,787–65,023,735) hold `to_muxed_id = NULL`                        | Targeted backfill after #674 deploys: `backfill-runner run --start N --end N --only asset_transfers` per ledger, run by the owner; check count 219,552 → 219,723                                                                          | 2026-10-19 |
| Muxed resolution for transfers is split between the parser (`asset_transfers.rs`) and persist (`value_flow.rs`)            | Moves into the parser: it returns canonical `G…`/`C…` ends with resolved `from_muxed_id`/`to_muxed_id`; persist only maps surrogates. Own `[refactor]` PR after #674, proven by unchanged outputs on real ledgers                         | 2026-10-19 |

## Not yet decided

- Nothing open.

## Tests per decision row

| Row                      | Tests                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `W…` full support        | `value_flow/tests.rs` `a_w_address_in_the_topic_is_split_into_its_contract_and_id`; `api` `common/path/tests.rs` `contract_id_*`, `common/filters.rs` `contract_id_opt_accepts_c_and_w_and_rejects_the_rest`, `search/classifier.rs` `classifies_a_w_strkey_as_its_contract`, `common/strkey/tests.rs`; web `strkeyDecode.test.ts`, `ExecutionTrace.test.ts` `shows a W-address as itself…`                                                                  |
| Muxed ids in the parser  | the `value_flow/tests.rs` muxed tests now decode real events through `extract_asset_transfers`, assertions unchanged; a scratch diff of `asset_transfers` rows against `develop` on mainnet ledgers (PR body)                                                                                                                                                                                                                                                |
| Soroban recipient id     | `value_flow/tests.rs` `a_soroban_transfer_takes_the_recipients_id_from_its_event`, `a_classic_transfer_ignores_the_event_id_which_may_be_the_memo`; `asset_transfers/tests.rs` `a_text_to_muxed_id_is_not_an_id`                                                                                                                                                                                                                                             |
| `M…` like `W…`           | `api` `path/tests.rs` `account_id_*`, `filters.rs` `account_id_opt_accepts_g_and_m_and_rejects_the_rest`, `classifier.rs` `classifies_an_m_strkey_as_its_account`, `strkey/tests.rs` `an_m_strkey_names_its_account`; ui `strkey.test.ts`, `validators.test.ts` `names the page of the address a muxed key stands for`; web `AccountDetailPage.test.tsx` / `ContractDetailPage.test.tsx` redirects, `TransactionsListPage.test.tsx` `routes a muxed search…` |
| Executor on any protocol | `contract-executor` `a_real_token_answers_like_the_network` (entries of protocol-29 ledger 64,818,615 on host 30); the type has no protocol field left to pass                                                                                                                                                                                                                                                                                               |
| prices-api merge timing  | process, no test                                                                                                                                                                                                                                                                                                                                                                                                                                             |

## Out of scope

- Galexie protocol-30 image: task 0650 (follows 0605).
- Storing `closeTimeMs`: nobody asked for it; `closed_at` stays seconds.

## Acceptance Criteria

- [x] Workspace on `stellar-xdr` 30; the executor answers on protocol-29 and
      protocol-30 ledgers.
- [x] Workspace tests and clippy green (one failure also on `develop`:
      `pool_reserves_reconciliation`, pool `CAZ6W4WH…`, data not code).
- [x] Decode check of recent mainnet and testnet ledgers recorded above.
- [x] Each decision row has a test (table above).
- [x] **Docs updated** — PR 1: `xdr-parsing-overview.md` (executor runs at
      the host's protocol), `infrastructure-overview.md` (upgrade bumps
      `soroban-env-host` too); PR 2: `backend-overview.md` (`W…` accepted),
      `database-schema-overview.md` and `xdr-parsing-overview.md`
      (recipient id from the event); `[refactor]` PR: the same two (ids
      resolved in the parser).
- [x] **API types regenerated** — PR 1: no diff; PR 2: parameter
      descriptions.
