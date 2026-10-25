---
title: 'Protocol 30: XDR diff, code impact, decode check'
type: research
status: mature
---

# Protocol 30: facts behind task 0649

**What protocol 30 changes in the XDR** (measured: structural diff of
`xdr-definitions-json/xdr.json`, `stellar-xdr` 28.0.1 vs 30.0.0). All changes
are additive — no discriminant changed, no arm removed:

| Change                                                                                                                           | Shape                                                                                    | CAP                 |
| -------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | ------------------- |
| `ScAddressType::MuxedContract = 5`, `ScAddress::MuxedContract`                                                                   | `{ id: u64, contract_id }`, renders as a 69-char `W…` strkey                             | 0084                |
| `StellarValueType` 3, 4 and `StellarValueExt` arms `SignedMs`, `EmptyTxSetMs`                                                    | carry `closeTimeMs`; `closeTime` stays whole seconds (`closeTime == closeTimeMs / 1000`) | 0088                |
| `ContractCostType` 86–94                                                                                                         | ML-DSA cost types; no new `HostFunction`                                                 | 0087                |
| `SC_SPEC_TYPE_NAME_LIMIT = 1024`; spec UDT names `string<60>` → `string<1024>`; `ScSpecEventV0.name` `ScSymbol` → `string<1024>` | wire-compatible (same string encoding)                                                   | not traced to a CAP |

In 28.0.1 the muxed-contract arm existed only behind the non-default feature
`cap_0084_muxed_contract`; 30.0.0 removes the feature.

**Crates** (source: crates.io API, 2026-10-19). `soroban-env-host` 30.0.0
(2026-10-16) → `soroban-env-common =30.0.0` → `stellar-xdr =30.0.0`
(2026-10-08). There is no `stellar-xdr` 29. Both 30.0.0 crates depend on
`stellar-strkey ^1.0.0`. `soroban-env-host` 29.0.2 still pins
`stellar-xdr =28.0.0`. Our pin `soroban-env-host =29.0.0` is yanked
(29.0.1 and 29.0.2 replace it).

**`stellar-strkey` 1.0.0** (measured, source diff): adds
`Strkey::MuxedContract`; removes `Strkey::PrivateKeyEd25519`; splits
`DecodeError::Invalid` into nine variants. 0.0.16 cannot parse a `W…`.

**What the bump breaks in our code** (measured on the bumped tree).

- `cargo check --workspace --all-targets`: one error, the exhaustive match in
  `credentials_signer` (`crates/xdr-parser/src/op_source.rs`). Adding
  `ScAddress::MuxedContract(_)` to the contract arm (`None`, no human
  deployer) makes the workspace compile; clippy `-D warnings` is green.
- `cargo test -p contract-executor`: 2 of 4 fail with
  `HostError: Error(Context, InternalError)`. The host refuses any ledger
  whose protocol differs from its own: `check_ledger_protocol_supported`
  (`soroban-env-host-30.0.0/src/host.rs:572`, "ledger protocol version too
  old for host", disabled only under `cfg(test)` of the host crate itself).
  The same tests with the fixture ledger set to protocol 30 pass 4 of 4.
- Consequence: the indexer calls `call_view` with each ledger's own protocol
  (`crates/indexer/src/contract_metadata.rs:62`). With host 30 on a
  protocol-29 network every call is `Failed`, so new tokens get no
  `decimals`/`name`/`symbol` until the network votes; with host 29 after the
  vote, the same. stellar-core avoids this by linking two hosts (current and
  previous protocol).

**Where a `W…` address would flow after the bump** (read):

- Rendered as raw `W…` by `Display`: `scval_to_typed_json`
  (`ScVal::Address`), `balance_key_holder` in `ledger_value.rs`.
- Dropped as "not a contract" by `_ => None` / `let-else`: call-target
  decoding in `invocation.rs`, `balance_key_holder`, `contract_instance.rs`,
  `persist/ids.rs`, the `G…`/`C…` filters in `persist/stage.rs` and
  `contract_activity.rs`.
- `split_muxed` in `persist/value_flow.rs` handles only `M…`; a `W…` would be
  stored with kind `W` and no muxed id.
- API: path and filter parsers accept only 56-char `G…`/`C…` (`400` for
  anything else); search classifies only `G`/`C`/`L`/hash.
- No `FixedString(56)` in `init.sql`; addresses are `String` or `cityhash64`
  surrogates, so a 69-char value fits every column.
- No `W…` exists on any network until the protocol-30 vote. Ledger keys
  (contract data, balances) keep the plain contract address. Not verified:
  whether a `W…` can appear in an event topic, as `M…` can today.

**Close time.** `closed_at` comes from `scp_value.close_time`
(`crates/xdr-parser/src/ledger.rs`), still whole seconds under protocol 30.
Consecutive ledgers may share a second (CAP-0088); the API's `closed_at`
predicates are range filters (`>=`, `<`), which tolerate ties.

**Decode check** (measured, 2026-10-19). A scratch example ran the indexer's
pure `parse_ledger` on each ledger and wrote its `Debug` output, built once on
28 and once on 30:

| Network | Ledgers                                                                                | Protocols          | Source                                     |
| ------- | -------------------------------------------------------------------------------------- | ------------------ | ------------------------------------------ |
| mainnet | 65,021,700–65,021,999 + 50 each from 50,457,424 / 56,000,000 / 60,000,000 / 63,000,000 | 29, 20, 22, 24, 26 | public data lake (`aws-public-blockchain`) |
| testnet | 5,274,100–5,274,399                                                                    | 29                 | `getLedgers`, soroban-testnet.stellar.org  |

All 800 decode on 30. Output differs from 28 only in `parse_ms` (timing) and
in element order (the outputs are built from hash maps; two runs on 28 differ
the same way); with `parse_ms` dropped and lines sorted, 800 of 800 match.
What it cannot show: a protocol-30 ledger (none exists yet), a value swap
between lines (sorted-line comparison), and protocols 21, 23, 25, 27, 28.

**Downstream.** stellar-prices-api depends on our `xdr-parser` by git
(`branch = "develop"`) and pins `stellar-xdr =28.0.0`. Its `Cargo.lock`
pins a fixed revision, so merging here breaks its build only when it next
updates `xdr-parser`; from then on it must move to `stellar-xdr` 30 in the
same change.

**Not in this task.** Galexie: protocol 30 needs a Galexie image with captive
core 30 (the 0605 failure mode). On 2026-10-19 Docker Hub's newest tagged
`stellar/stellar-galexie` is 29.0.0. stellar-core v30.0.0 was released
2026-10-16; mainnet and testnet validators already run it. No vote date for
testnet or mainnet is announced.
