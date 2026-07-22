---
id: '0431'
title: 'REFACTOR/TEST: use the stellar-xdr API we already depend on +differential oracle against the official CLI'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0430', '0380', '0088', '0406']
tags:
  [priority-high, effort-medium, layer-xdr-parsing, layer-testing, correctness]
links:
  - crates/xdr-parser/src/op_source.rs
  - crates/domain/src/enums/operation_type.rs
history:
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      Spawned after 0430. The deployer bug survived two months of production
      despite a dedicated module, a validation task, and 890 tests — because
      every test compares our code against our own expectations, and the
      library already had the thing we hand-rolled.
      We depend on `stellar-xdr = 27.0.0`; the official `stellar` CLI is the same
      crate at 26.0.0. So the CLI is NOT a better parser — it is the same decoder
      exposed as a binary. Our defect is one layer up, in extraction: we reach
      into the envelope by hand and stop one level short on fee-bump nesting.
      Sweep findings below are measured, not guessed.
---

# Use the `stellar-xdr` API we already pay for, and pin it with a differential oracle

## Why now

0430: `deployer_id` stores the wrong account on fee-bump envelopes. The library
ships **`tx_auths.rs`** with `TransactionEnvelope::auths()` **and** a separate
`FeeBumpTransactionEnvelope::auths()` — an iterator that flattens operations to
`SorobanAuthorizationEntry` and handles both envelope shapes. We hand-rolled the
traversal and missed the fee-bump case. The bug was avoidable by using what we
already compile.

## A. Library surface we depend on and never call

`stellar-xdr` 27.0.0 ships these helper modules. Grep says **zero call sites**
for all four:

| module                                                 | lines   | what it gives                                                        | what we do instead                                                                                       |
| ------------------------------------------------------ | ------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `tx_auths.rs`                                          | 168     | `auths()` over both envelope kinds → `SorobanAuthorizationEntry`     | hand-rolled traversal; **caused 0430**                                                                   |
| `scval_conversions.rs`                                 | 817     | `From`/`TryFrom` between `ScVal` and Rust types                      | our own `scval.rs` (311 lines)                                                                           |
| `str.rs`                                               | 531     | `Display`/`FromStr` for `PublicKey`, `AccountId`, `ContractId`       | scattered manual strkey handling                                                                         |
| `num256.rs`                                            | 54      | `u256_str_from_pieces` / `i256_str_from_pieces`                      | **0380 exists to build exactly this**                                                                    |
| `tx_hash.rs`                                           | 182     | `TransactionEnvelope::hash(network_id)` for all three envelope kinds | **our own `tx_envelope_hash`** in `envelope.rs` — hand-promotes V0→V1, builds the tagged payload, hashes |
| `ledgerkey.rs`                                         | 187     | `LedgerEntry::to_key()` / `LedgerEntryData::to_key()`                | hand-built key matching in `ledger_entry_changes.rs`                                                     |
| `num128.rs`                                            | 26      | `u128`/`i128` ↔ string                                               | `decimal7_string_to_i128` in `stage.rs`, `parse::<i128>` in `api`                                        |
| `scval_validations.rs`                                 | 180     | `Validate` trait for `ScVal` / `ScMap`                               | none — we do not validate                                                                                |
| `scmap.rs`                                             | 75      | `ScMap::sorted_from_*` (spec requires sorted maps)                   | unknown                                                                                                  |
| `transaction_conversions.rs`, `account_conversions.rs` | 96 + 25 | `From` impls between envelope/account types                          | hand-written conversions                                                                                 |

Two stand out.

**`tx_hash.rs` is the risky one.** We hand-rolled `tx_envelope_hash`
(`crates/xdr-parser/src/envelope.rs:111`): it promotes a V0 envelope to V1 field
by field, constructs `TransactionSignaturePayloadTaggedTransaction`, then
hashes. The library does exactly this, tested by SDF, for all three envelope
kinds. This is cryptographically load-bearing code — a transaction hash that
disagrees with the network is not a cosmetic bug. Note that right next to it
sits `unmatched_hash()`, a sentinel for "task-0190 unmatched envelope hash",
i.e. hash-matching failures have already happened in this codebase.

**`num256` is the most wasteful.** Task 0380 ("u256/i256 decoded, not raw hex")
is queued work for a function that already exists upstream.

## B. Hand-maintained copies of XDR definitions

`crates/domain/src/enums/operation_type.rs` re-declares all 27 `OperationType`
variants and states in a comment: _"Discriminants mirror
`stellar_xdr::OperationType` byte-for-byte"_. But `crates/domain` **does not
import `stellar_xdr` at all** — the only occurrence is that comment. Nothing
enforces the claim.

Checked today: **27 variants ours, 27 in the crate — currently aligned.** The
risk is not present drift, it is silent future drift when a protocol version
adds an operation.

Note the copy is not pointless: `domain` stays dependency-light and the enum
carries serde/utoipa derives the XDR type lacks. The fix is probably not
deletion but a **compile-time or test-time equality assertion** against
`stellar_xdr::OperationType`, so drift fails the build instead of shipping.

## C. Differential oracle (the test half)

**What it does.** Take real envelopes off the network, decode each twice — once
through our parser, once through `stellar xdr decode --type TransactionEnvelope
--output json` — and assert that every field we surface matches the official
decode. We do not have to surface everything; what we do surface must agree.

**Why it catches what 890 tests did not.** Existing tests assert our output
against fixtures we wrote. A fixture built from a plain envelope can never
exercise fee-bump nesting, and nobody knew to write one — that is exactly how
0430 hid for two months. An oracle built from _production_ envelopes has no such
blind spot: whatever shapes mainnet produces, the corpus contains.

**Concretely for 0430:** the oracle sees `op.source_account = GCNP4JVZ…` in the
CLI output and `GB7CY43V…` in ours, and fails — in May, not July.

**What it will NOT catch.** Anything above parsing: wrong aggregation, wrong
attribution, wrong classification. It answers exactly one question — "did we
read out of the XDR what is actually in it".

**Cost shape.** No protocol modelling, no decoder — those exist. The work is a
comparison harness plus a corpus. One-off cost; value compounds with every new
envelope shape added.

## Implementation

- [ ] Put the oracle in its own directory (e.g. `crates/xdr-parser/tests/oracle/`)
      — separate corpus + harness from the unit tests. Different purpose,
      different runtime, different reason to fail; mixing them makes both harder
      to reason about.
- [ ] Verify `tx_envelope_hash` against `TransactionEnvelope::hash()` on the
      corpus BEFORE replacing it — if they already disagree anywhere, that is a
      live bug, not a refactor.
- [ ] Corpus: N real envelopes pulled via `getTransaction` on Soroban RPC,
      deliberately covering plain v0/v1, fee-bump, per-op source override,
      factory deployment, multi-auth. Store base64 as fixtures (RPC retention is
      ~7 days — the corpus must be committed, not fetched at test time).
- [ ] Harness: decode each fixture with `stellar xdr decode` (or the same crate
      API directly — same code, no subprocess) and diff against our parser's
      output on the fields we emit.
- [ ] Wire into CI. Note 0406: the ClickHouse-gated suite never runs; this
      harness must not land in the same silent-skip trap — it needs no database,
      so it can run unconditionally.
- [ ] Replace the hand-rolled auth traversal with `tx_auths::auths()` (this is
      also 0430's fix, coordinate).
- [ ] Assert `domain::OperationType` against `stellar_xdr::OperationType` so
      drift fails the build.
- [ ] Survey the remaining helper modules and record which are worth adopting —
      `num256` should be handed to 0380 rather than duplicated there.

## Acceptance Criteria

- [ ] Oracle runs in CI unconditionally and fails on a seeded mismatch
      (prove it catches something, don't just assert it passes).
- [ ] The 0430 fee-bump case is in the corpus and passes after 0430 lands.
- [ ] `OperationType` drift is a build/test failure, not a silent divergence.
- [ ] A written verdict per helper module: adopt, or keep ours with a reason.
- [ ] Docs updated — `N/A` (test infrastructure; no architecture shape change).
- [ ] API types regenerated — `N/A`.

## Explicitly out of scope

Replacing our parser with the CLI. The CLI is a process, not a library; at 13M
ledgers, spawning one per transaction is not viable. We already link the same
crate — the goal is to _call more of it_, not to shell out.
