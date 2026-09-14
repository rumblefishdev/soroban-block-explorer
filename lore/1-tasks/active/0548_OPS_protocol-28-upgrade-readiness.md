---
id: '0548'
title: 'Protocol 28 (Adapter) readiness: Galexie 28.0.1 pin + stellar-xdr 27→28 before the 2026-09-16 pubnet vote'
type: OPS
status: active
related_adr: []
related_tasks: ['0367', '0368']
tags: [indexer, xdr, infra, galexie, protocol-28, priority-high]
links:
  - 'https://stellar.org/blog/developers/adapter-protocol-28-upgrade-guide'
  - 'https://stellar.org/blog/developers/introducing-adapter-protocol-28-on-stellar'
  - 'https://hub.docker.com/r/stellar/stellar-galexie/tags'
history:
  - date: 2026-09-08
    status: active
    who: karolkow
    note: >
      Created 8 days before the mainnet vote, from the SDF upgrade
      announcement. First PLANNED execution of the protocol-upgrade bump —
      0367/0368 were both reactive (16 h silent ingestion stall, then a
      7.5k-message DLQ). 0367's own future-work list called for exactly this.
---

# Protocol 28 (Adapter) readiness

## Summary

Pubnet votes to protocol 28 on **2026-09-16 17:00 UTC**. Two independent things
break at that instant if untouched: the digest-pinned Galexie image still ships a
protocol-27 captive core (stops writing to S3 → ingestion starves), and the
workspace still pins `stellar-xdr = "27"`, which cannot decode the new XDR union
arms (parse error → DLQ). Both failures already happened once, on the 26→27
upgrade — this task is the same work done _before_ the vote instead of after.

## Context

Repeat of the 2026-07-08/09 incident pair:

- **0367** — Galexie ran a pre-27 core through the protocol-27 vote. Captive core
  kept following SCP but could not apply new-protocol ledgers
  (`History: Skipping catchup: incompatible core version`), stalled mid-sync and
  wrote nothing to S3. Ran ~16 h before a human noticed. Galexie's version tracks
  the protocol, so **28.0.x == protocol 28**.
- **0368** — the indexer binary linked `stellar-xdr 26`, so the first proto-27
  ledger (63401875) failed `HandlerError::Parse`; the reconcile aborted before
  commit and the SQS doorbell redelivered until it dead-lettered (~7.5k messages).

What is different this time: we have advance notice, so both halves are planned
work rather than an outage. 0367's _Future Work_ listed "protocol-upgrade watch
process … so the Galexie/core bump is a planned ~20-min task before each pubnet
upgrade, not an outage" — this task is that process firing for the first time.

### Protocol 28 content (three CAPs, two touch us)

- **CAP-83 — empty transaction set consensus.** New `STELLAR_VALUE_EMPTY_TX_SET`
  arm on the `StellarValue.ext` union inside `LedgerHeader.scpValue`, with
  `txSetHash` all-zero. **No new `LedgerCloseMeta` version** — the container stays
  V2. We only read `scp_value.close_time` (`crates/xdr-parser/src/ledger.rs:18`),
  so there is no logic change; the exposure is purely that stellar-xdr 27 cannot
  decode an unknown union arm, and `deserialize_batch`
  (`crates/xdr-parser/src/lib.rs:111`) fails the **whole batch**, not one ledger.
- **CAP-85 — external contract executables.** New
  `CONTRACT_EXECUTABLE_EXTERNAL_REF` arm on `ContractExecutable`: a contract's
  code can now be a reference to an executable owned by _another_ contract, so an
  admin can upgrade a whole fleet atomically. Breaks the assumption that a
  contract instance carries its own `wasm_hash` — see Future Work.
- **CAP-86 — sparse map host functions.** Host functions only, no XDR change.
  No impact here.
- **A fourth XDR change, not attributed to any of the three CAPs above.** A
  diff of the `.x` sources (`stellar-xdr` v27.0 → v28.0) shows the whole wire
  change lives in exactly two files, and it carries one arm the CAP write-ups do
  not mention: **`SCV_EXECUTABLE_TAG = 22`** on `SCValType`, carrying an
  `SCString executable_tag`. It is the read side of CAP-85 — a contract names
  which executable of the owner it wants — but it lands on the `ScVal` match,
  not the `ContractExecutable` one, so it is a separate break site (see Blast
  radius). Also in the diff, wire-neutral: `SCBytes`/`SCString`/`SCSymbol`
  typedefs were moved earlier in the file, and `rentFeeCharged`'s comment was
  corrected — it is part of `totalRefundableResourceFeeCharged`, not the
  non-refundable one. We read neither field, so no exposure.
- **Not in protocol 28, despite being in the file:**
  `SC_ADDRESS_TYPE_MUXED_CONTRACT` sits behind `#ifdef CAP_0084_MUXED_CONTRACT`
  in v28.0 and is not compiled into the release. Do not plan for it here.
- **Verified against the wire format, not the announcement:**
  `LedgerCloseMeta` in v28.0 still switches on `v0`/`v1`/`v2` only — the "no new
  container version" claim above is confirmed at the source, not inferred.

### Blast radius in this workspace

`stellar-xdr` is pinned once, at `Cargo.toml:40`, and used by 7 crates
(xdr-parser, api, db-clickhouse, indexer, enrichment-shared, backfill-runner,
audit-harness). The new `ContractExecutable` arm lands on three **exhaustive**
matches in production code, so the bump fails at compile time rather than
silently rendering the wrong thing:

- `crates/xdr-parser/src/scval.rs:77`
- `crates/xdr-parser/src/operation.rs:835`
- `crates/xdr-parser/src/invocation.rs:558`

(`crates/xdr-parser/src/token_metadata.rs:87` compares with `==` against
`StellarAsset` and is unaffected.)

**Plus a fourth site for the new `ScVal` arm.** `scval_to_typed_json`
(`crates/xdr-parser/src/scval.rs`, the `ScVal` match ending at
`ScVal::LedgerKeyNonce`) has no `_ =>` arm, so `SCV_EXECUTABLE_TAG` also fails
the build there — the same loud failure, but a site the CAP-85 write-up does not
point at. The other `ScVal` matches (`ledger_value.rs`, `nft.rs`, `sac.rs`,
`token_metadata.rs`) carry `_ =>` fallbacks and compile silently — checked by
hand afterwards, and they are narrow shape matchers (balance keys, metadata
maps) that an executable tag can never reach. The compiler-invisible damage
turned out to be elsewhere entirely; see "What the compiler could NOT tell us".

Unlike 26→27, no module reshuffle is expected: the `curr`/`next` split was
already removed in 27, so this should be a pin bump plus the new match arms.

Not affected: Soroban RPC is SDF-public (`mainnet.sorobanrpc.com`,
`DEFAULT_SOROBAN_RPC_URLS`, `crates/enrichment-shared/src/nft_token_uri/client.rs:43`) — SDF upgrades it. No
`@stellar/*` JS dependency exists in the frontend. We run no validators, so the
2026-09-09 validator-arming deadline does not apply.

## Implementation Plan

### Step 1 — Galexie image (infra; must land before 2026-09-16 17:00 UTC)

`stellar/stellar-galexie:28.0.1` was published 2026-08-27 (28.0.0 on 2026-08-14).
Confirmed at Docker Hub on 2026-09-10: 28.0.1 is still the newest release tag,
it is also what `latest` points at, and its (single, linux/amd64) source digest
is `sha256:1d511631693274eba3f44e23c9411c12c877f573816ae506804f611694dcf01b`.

Two things worth knowing before pinning:

- **Do not pin a newer digest just because one exists.** Hub also carries
  commit-SHA tags pushed after the release (`51e846a` 08-28, `11ffa48` 09-03,
  `2aa7c4a` 09-09). Those are master builds, not releases. 28.0.1 is the tag.
- **What is actually inside the image.** `docker/Dockerfile` takes the core
  version as a build arg; the release workflow at tag `galexie-v28.0.1` sets
  `STELLAR_CORE_VERSION: 28.0.1-3508.947aad841.noble`. So the image ships core
  28.0.1 even though the core v28.0.1 GitHub release was published 2026-09-01,
  after the image was built — the Debian package landed first. Core 28.0.1 over
  28.0.0 is three stability fixes (query-message dedup, skipping background tx
  signature verification for unauthenticated peers, clamped tx-set fee sums),
  none of them protocol behaviour. The proto-28 support itself came with
  Galexie 28.0.0 / core 28.0.0.

Mirror it into ECR, read the **landed ECR digest back**, and pin that:

- `galexieImageTag` in `infra/envs/production.json:24`
  (currently `sha256:91eae7af…3c82c8` = the Galexie 27.0.0 mirror from 0367)
- GitHub env `GALEXIE_IMAGE_DIGEST` (production + staging) → the **Docker Hub**
  source digest

Per 0367: the ECR digest differs from the Hub digest because `docker push`
re-serialises the manifest, and `galexieImageTag` is resolved via
`fromEcrRepository`, so it must hold the ECR one. Read it back with
`aws ecr batch-get-image … imageId.imageDigest`, never reuse the Hub digest.

Deploy is the user's (`aws-admin` shell). Budget ~20 min of warmup on the
restart: a fresh Fargate task has empty ephemeral storage and re-downloads and
applies ~16 GB of BucketList state before it resumes exporting.

### Step 2 — stellar-xdr 27 → 28 (code)

1. `Cargo.toml:40` → `stellar-xdr = { version = "28" }` (28.0.0 is on crates.io,
   published 2026-07-30).
2. Handle `ContractExecutable::ExternalRef` at the three sites above. Render it
   honestly — an explicit `external_ref` type carrying the owner reference, never
   a fallback that makes it look like a plain `wasm` executable
   (a plausible-but-wrong render is worse than a loud failure).
3. `cargo build --workspace --all-targets` + `cargo test -p xdr-parser` to catch
   any field-level struct change beyond the new arms.
4. Regenerate api-types — `Cargo.{toml,lock}` change trips the
   `API types freshness` CI gate: `npx nx run @rumblefish/api-types:generate`.

### Step 3 — verify decode before the vote

Testnet is **already on protocol 28** — `getLatestLedger` against
`soroban-testnet.stellar.org` returned `protocolVersion: 28` at ledger 4566692
(checked 2026-09-08; mainnet returned 27 at ledger 64329242 the same second). So
proto-28 ledgers exist to test against today. Decode one as a positive control
rather than waiting for mainnet to prove it.

### Step 4 — deploy + watch

Deploy compute after the vote window; watch `production-galexie-ingestion-lag`
(the alarm 0367 fixed: SQS `NumberOfMessagesSent`, 5-min window,
`treatMissingData: BREACHING`) and the DLQ depth across 17:00 UTC.

## Progress — Step 2 done on 2026-09-10, six days ahead of the vote

Branch `ops/0548_protocol-28-upgrade-readiness`. `stellar-xdr` is at 28.0.0 and
`cargo check --workspace --all-targets` is green.

**The compiler's own list of break sites, which is four, not three.** The first
build against 28.0.0 failed with exactly:

| Site                                      | Missing arm                          |
| ----------------------------------------- | ------------------------------------ |
| `crates/xdr-parser/src/scval.rs:16`       | `ScVal::ExecutableTag(_)`            |
| `crates/xdr-parser/src/scval.rs:76`       | `ContractExecutable::ExternalRef(_)` |
| `crates/xdr-parser/src/operation.rs:834`  | `ContractExecutable::ExternalRef(_)` |
| `crates/xdr-parser/src/invocation.rs:557` | `ContractExecutable::ExternalRef(_)` |

Nothing downstream broke — the other six crates that use `stellar-xdr` compiled
untouched, and no module reshuffle happened, as expected.

**How the new arms render.** `{"type":"external_ref","owner":<C… StrKey>,"tag":<string>}`
and `{"type":"executable_tag","value":<string>}`. Deliberately NOT folded into
`wasm` with a borrowed or zeroed hash: a consumer has to be able to tell a
contract that carries its own code from one that points at somebody else's.

**Tests.** `crates/xdr-parser/tests/protocol_28_arms.rs` (5 tests) plus one
inline test each in `operation.rs` and `invocation.rs` for their private
renderers. The suite is 420 + 5 passing, `cargo fmt` and `clippy` clean.

**The positive control is real chain data, not a hand-built value.** Testnet
ledger 4,601,991, pulled from `soroban-testnet.stellar.org` via `getLedgers` on
2026-09-10 and committed as
`crates/xdr-parser/tests/fixtures/testnet_p28_ledger_4601991.b64` (66 KB). It
decodes through `extract_ledger` and reports `protocol_version = 28`. Testnet
carried no empty-tx-set ledger to capture — 40 consecutive ledgers scanned, the
smallest still 50 KB of meta — so CAP-83 is covered by a constructed
`StellarValueExt::EmptyTxSet` round-trip instead, which is the arm that fails to
decode under 27.

**Traced downstream, one finding.** `extract_wasm_hash`
(`crates/xdr-parser/src/state.rs:367`) reads `executable.hash`, so an
external-ref contract yields `wasm_hash = None`. The deployment row is still
written (`contract_type = Other`), so nothing silently vanishes — but the code
hash will read as empty with no explanation of why. That is the display half of
the CAP-85 item already in Future Work, not a new defect.

**Not done here:** the Galexie image (Step 1), the deploy (Step 4), and the
sibling `prices` repo. All three are the user's.

**Tests extracted to siblings.** `operation.rs` 1,707 → 848 lines and
`invocation.rs` 1,606 → 571, with 859 and 1,035 lines of tests moved into
`operation_tests.rs` / `invocation_tests.rs` via the `#[path]` form `event.rs`
already uses. `operation.rs` is still ~48 lines over the limit; splitting it by
topic is separate work.

## What the compiler could NOT tell us

The bump makes the decode safe. It does not make the SEMANTICS right, and the
two are not the same thing: every place we read an executable out of **decoded
JSON** rather than out of a Rust enum kept compiling and quietly changed
meaning. Both gaps below are the same shape — a positive claim that is now
wrong, not a missing value.

**Both gaps are now closed, by the model below.** Kept here because the
reasoning is the argument for that model, not a historical note.

**Gap 1 — an external-ref upgrade leaves the stored hash stale.**
`update_current_contract_executable_ref` emits the SAME `executable_update`
event as a Wasm upgrade (stated in CAP-85 itself), with the new executable as
`vec[Symbol("ExternalRef"), map{owner, tag}]`, and a contract may move freely
between a direct Wasm hash and a reference.
`extract_executable_update_new_wasm_hash` (`crates/xdr-parser/src/event.rs`)
tests for `Symbol("Wasm")` and returns `None` for anything else, so the caller
in `persist/stage.rs` skips the row and `soroban_contracts.wasm_hash` keeps the
hash from BEFORE the upgrade. That is the stale-hash defect of 0320/0326,
reappearing through a door the compiler cannot watch. Pinned by a test in
`event_tests.rs` and documented on the function; not fixed, because what the
column should hold for a fleet member is a data-model decision.

The hash is not lost, for whatever we decide: CAP-85 guarantees the owner keeps
a persistent contract-data entry, keyed by the executable tag, whose value is
the 32-byte hash of a real `ContractCode`. That key is an `SCV_EXECUTABLE_TAG`
value — which the new `ScVal` arm now decodes, so those entries are already
readable in `ledger_entry_changes`.

**Gap 2 — the upgradeable chip will assert the opposite of the truth.**
`map_upgradeable` (`crates/api/src/contracts/queries.rs:515`) returns
`Some(false)` — a positive "cannot self-upgrade" — for any contract with
`wasm_hash IS NULL`, on the reasoning that no WASM means a SAC. An external-ref
contract also has no `wasm_hash`, and is the _most_ upgradeable kind there is:
its owner re-points the whole fleet at once. The honest interim value is `None`
(no chip).

Unlike gap 1 this one is cheap to close: `is_sac` is already selected in the
same row (`queries.rs:493`), so "no hash **and** SAC" can be told apart from
"no hash **and not** SAC" without storing anything new. The first keeps its
hard `Some(false)`; the second stops guessing. That also makes external-ref
contracts default to silence rather than to a false claim, which buys time for
the model decision.

**What the null-hash population actually is** (measured on production
2026-09-10, so the fix is not argued from a wrong premise). Of 148,970
contracts: 144,908 carry a hash, 4,008 are SACs, and **54** have no hash and
are not SACs. Those 54 are all Pass-2 stubs — `deployed_at_ledger = 0`, every
identity column NULL.

They are **not** deploys we missed. Our floor is ledger 50,457,424, closing
2024-02-20 17:00:10 — the protocol-20 vote itself — so no Soroban contract can
predate our indexing. Three of the 54 were probed against mainnet by building
the instance `LedgerKey` by hand and calling `getLedgerEntries`: none has an
instance on chain, while a control contract returns one. At least two are
deterministic SAC addresses of classic assets carrying `sac_deployed = 0` in
`asset_sac` — addresses that exist by derivation and get referenced, but where
no contract was ever deployed. Only three of the 54 appear in any event or
operation at all.

So the damage today is 0.04% of rows, mostly addresses with nothing behind
them. The reason to fix the inference is not those 54 — it is that every
external-ref contract lands in exactly that bucket.

**Gap 2 is closed** (`map_upgradeable` now takes `is_sac`). A SAC keeps its
hard `Some(false)`; anything else without a hash returns `None` and the chip
stays off. Two tests, one per population.

### The model that closed them (option C, chosen 2026-09-10)

Three facts, three homes, each written by the ledger change that actually
changes it:

| Fact                          | Where                                                                        | Changes when                        |
| ----------------------------- | ---------------------------------------------------------------------------- | ----------------------------------- |
| which kind of executable      | derived from `is_sac` / `wasm_hash` / `executable_owner_id` — no enum column | the contract is created or upgraded |
| the reference it states       | `soroban_contracts.executable_owner_id` + `executable_tag`                   | as above                            |
| what that reference points at | `contract_executable_refs (owner_id, tag) → wasm_hash`                       | the OWNER writes its entry          |

"which code does this contract run" is then a join at read time. That is also
what the protocol itself answers: CAP-85 has `get_address_executable` resolve
the reference and return the real hash, explicitly rejecting returning the
reference because it "would not be helpful" to a caller who cannot resolve it.

The rejected alternative was storing the resolved hash on each member. It reads
the same but writes catastrophically: one owner write would force a rewrite of
every member, and an interrupted rewrite leaves half a fleet on each hash with
nothing to say which is right. `(owner_id, tag)` is the key because a tag is
unique only within its owner — the same shape as `(asset_code, issuer)` for a
classic asset.

What landed:

- `crates/xdr-parser/src/executable_ref.rs` — reads the reference off an
  instance, and lifts `(owner, tag) → hash` out of the owner's contract-data
  entries (the key is protocol 28's new `SCV_EXECUTABLE_TAG` `ScVal`, which is
  why the pin bump had to come first). 7 tests.
- `event.rs` — `extract_executable_update_new_wasm_hash` became
  `extract_executable_update`, returning `Wasm(hash) | ExternalRef{owner, tag}`.
  The old signature could not express the second arm, which is exactly how the
  gap stayed invisible.
- `build_wasm_upgrade_rows` — each arm CLEARS what the other sets, so a
  contract moving either direction cannot keep a hash or a reference it no
  longer has. Two tests, one per direction.
- Read path — the contract detail and the decompiler/interface query both
  resolve through `argMax(wasm_hash, ledger)` (no `FINAL` on the RMT), and the
  DTO carries `executable_owner` + `executable_tag` so a consumer can still
  tell own code from borrowed.
- A LEFT JOIN miss in ClickHouse fills a `FixedString(32)` with **zeros, not
  NULL**, so the resolution is guarded on `ref.ledger != 0`. Without it a SAC
  would have been handed a hash of 64 zeros — a value that looks like a hash
  and is not.

**Two decisions taken after review (2026-09-10).**

- The owner was briefly stored as the `C…` StrKey, then reverted to the
  usual surrogate (`executable_owner_id`, like `deployer_id`) on review: the
  owner is itself a deployed contract, so its own row resolves the address,
  and the text column was an exception to the repo convention with no
  remaining correctness reason once that was clear. The refs table keys on
  `(owner_id, tag)` to match.
- `contract_addresses` splits the id → StrKey dictionary out of
  `soroban_contracts` (the S3 option). Pass-2 no longer writes placeholder
  contract rows at all: an address we have only seen mentioned gets a
  dictionary row and nothing else. That removes the fourth "unknown" executable
  kind — it was never a legal state of a contract, only an artefact of one
  table meaning two things — and takes the phantom rows out of the public
  contracts list and the network count. Every join that wanted only a StrKey
  (`resolve_contracts`, and the joins in accounts / assets / nfts / liquidity
  pools) now reads the dictionary; the accounts one also drops its `FINAL`,
  since a dictionary row is byte-identical per id and an unmerged duplicate
  cannot double a balance leg.

Re-measured on production while writing this: 39,342 placeholder rows, up from
37,950 earlier the same day — they accumulate continuously. Verified that none
of them carries identity data (`wasm_hash` / `is_sac` / `deployer_id` all NULL
across all 39,342), so the `wasm_uploaded_at_ledger = 0` sentinel is a safe
predicate for the one-off cleanup.

**What writes to `soroban_contracts`, and what can overwrite a real row**
(traced 2026-09-11, because a placeholder was suspected of clobbering real
contracts). The table is a ReplacingMergeTree, so nothing is updated in place:
a newer-version row replaces the WHOLE row at merge.

- **Placeholders cannot overwrite a real contract.** They are written at version
  0 and a real row's version is its deploy ledger. Measured in one snapshot:
  41,010 placeholder rows — 36,246 contracts holding both a placeholder and a
  real row (pure duplicates), 54 holding only a placeholder (addresses merely
  mentioned). None of the placeholder rows carries identity data.
- **The WASM upgrade path does overwrite, on purpose.** It clones the prior row,
  changes the executable, and writes it at the upgrade ledger. 132 contracts on
  production have two real versions (maximum two). Because it clones, the new
  `executable_owner_id` / `executable_tag` columns ride along automatically.
- **`contract_type_rebuild` rewrites the whole table and swaps it in.** Its
  `INSERT … SELECT` named the columns explicitly and had not been told about
  the two new ones. Probed on a throwaway ClickHouse 26.3 database: the short
  list fails with `NUMBER_OF_COLUMNS_DOESNT_MATCH` before the `EXCHANGE`, so the
  failure mode was a broken operator command, not lost data. Fixed, and guarded
  by `staging_select_passes_every_column_through`, which reads the column list
  off the same row struct the writer uses.
- The other five `INSERT INTO soroban_contracts` found by grep are all test
  fixtures, each after its file's `#[cfg(test)]`.

**Cross-table review, 2026-09-11 — do the two defects found here exist elsewhere?**

_Same-ledger version ties._ ClickHouse's documented rule: on equal version,
"the most recent inserted row will remain" — physical insert order, not chain
order — and deduplication happens only at a merge, which is not guaranteed.
So every ledger-versioned table needs the writer to fold same-key rows before
insert. Checked on `develop`:

| table                                                                 | same-ledger tie handled by                           | verdict                                                                                                                                                        |
| --------------------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `liquidity_pools`, `lp_positions`, `balances`, `nfts`, `nfts_pending` | `>=` watermark compare in the stage map              | correct (later wins)                                                                                                                                           |
| `pool_state_changes`, `pool_instance_state`                           | `fold::keep_last_by_key`                             | correct, with corpus evidence in the comment                                                                                                                   |
| `accounts`                                                            | per-field fold (`merge_account_state_overrides`)     | correct                                                                                                                                                        |
| `account_entry_state`                                                 | map insert, last wins                                | correct                                                                                                                                                        |
| `assets`                                                              | version-less, identity-only rows                     | harmless                                                                                                                                                       |
| **`soroban_contract_metadata`**                                       | **nothing** — one row per change, `version` = ledger | **same defect**: two METADATA writes for one contract in one ledger leave the survivor to insert order. Unmeasured on production (read quota). Not fixed here. |

`contract_executable_refs` now folds with the same shared helper rather than a
hand-rolled map.

_Rows from a mere mention._ Only two writers do it. `soroban_contracts` Pass-2
stubs are removed by this task. `accounts` has the same shape by design: every
transaction participant gets an `accounts` row, and one never observed as an
`AccountEntry` change is a skeleton (`sequence_number = 0`) —
`backfill-runner/src/bootstrap.rs` exists to fill those from RPC. Different
consequence (a participant usually IS a live account, and `last_seen_ledger` is
an activity fact), so not the same defect, but the same question applies to a
payment towards an account that does not exist. Not changed here.

**Correction to the address/deployment split.** It was argued as removing
duplicate writes. It does not: `contract_addresses` gets a row for every
address seen in every batch, the same volume the stubs had, just no longer
disguised as contracts. What it genuinely buys is narrower — the transaction
page resolves contract surrogates through it, and for the 54 addresses with no
contract behind them (53 of them invocation targets, 0 of 54 with a live
instance on mainnet) no row means `unwrap_or_default()`, an empty address.
Whether to keep it is open (D5).

**Who actually translates a contract surrogate back into a StrKey, and why
the answer differs per reader (traced 2026-09-11).** Surrogates exist because
`soroban_contracts`, `accounts` and `transactions` are each referenced by 6–8
downstream tables, and ClickHouse joins and groups on an `Int64` in one CPU
operation where a 56-character StrKey costs a variable-length compare
(`persist/ids.rs`). The price is that every reader needing the text must look
it up. The readers, and what each one is really asking:

| endpoint                                         | reader                                                                                                | question it asks                         | may it see a non-contract address?                                                    |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------- |
| `/transactions/{hash}`                           | `fetch_operations`, `fetch_event_appearances`, `fetch_invocation_appearances` via `resolve_contracts` | "what address did this row name"         | **yes** — a call to an address with nothing behind it is a fact about the transaction |
| `/liquidity-pools`, `/liquidity-pools/{pool_id}` | SAC mirror of a pool leg                                                                              | "is there a DEPLOYED SAC for this asset" | **no** — `PoolAssetLeg.contract_id` is documented as `None` without a deployed mirror |
| `/accounts/{account_id}`                         | `BALANCES_SQL`, token contract of a balance                                                           | "what address is this token"             | tokens are always deployed, so no difference                                          |
| asset list search                                | `build_list_seek_sql`, token contract → metadata                                                      | same                                     | same                                                                                  |
| NFT list                                         | `FROM soroban_contracts` directly, not a join                                                         | "which NFT contract"                     | NFT contracts are always deployed; untouched                                          |

The split had pointed the pool joins at `contract_addresses`. Because
`asset_sac` also carries the surrogate of UN-deployed SACs, and the dictionary
knows every address seen, the pool leg would have started returning a contract
address for a SAC that does not exist on chain — against the documented API
contract. No visible change in the current frontend (`legHref` only reaches
`contract_id` when a leg lacks code and issuer, which classic and native legs
never do), but a real contract break for any other consumer. Restored to
`soroban_contracts`, with the reason written at both join sites.

**The read-path SQL did not run at all as first written (found 2026-09-11).**
Rust compiled it as a string and nothing ever executed it. Run against the
real `init.sql` definitions on a throwaway local ClickHouse 26.3 database,
both `fetch_contract` and `fetch_wasm_interface` failed on every contract
with `Code 184`: the refs subquery selected `argMax(wasm_hash, ledger) AS
wasm_hash, max(ledger) AS ledger`, and the alias `ledger` shadowed the column
inside `argMax`, which ClickHouse reads as an aggregate inside an aggregate.
Deployed, every contract detail page and every decompiler request would have
returned 500.

The fix also removed the `ref.ledger != 0` sentinel, which was the other smell:
the subquery now returns `toNullable(argMax(wasm_hash, ledger))`. A LEFT JOIN
miss fills a column with its type's default, which is NULL only for a Nullable
type — so a miss is honestly NULL without leaning on "ledger 0 cannot exist",
and `coalesce(sc.wasm_hash, ref.wasm_hash)` replaces the guarded `if`.
Re-verified on the same seeded tables: own-code contract → its hash, SAC →
NULL, fleet member → the NEWER of two refs rows, fleet member whose tag has no
row → NULL (not 32 zero bytes); the decompiler query resolves the ABI through
the referenced hash. A CH-backed regression test for these two queries does
not exist yet and is proposed, not written.

**D5, the concrete case.** `CDUQMUE7GNZRQLFF2OSK3577OVBKO2QHXTUTLMNGEHZVPGJCGTL57RAO`
has no contract row, no live instance on mainnet, and is not a SAC. Failed
transaction `d5b29b00178940cb73a2c0ad6e803693ede33d3a2c9672ffa12e0e9460fc3078`
(ledger 58,541,598) called it. On `/transactions/{hash}` the frontend builds
the operation headline from the DB-sourced `operations[].contract_id`
(`web/src/pages/transaction-detail/shared/humanizeOp.ts`), which resolves
through `resolve_contracts`. With a row for the address: "Called fn() on
CDUQ…". Without one the API returns `null`, and the headline silently drops
the address. The heavy call tree still shows it as text when the public-archive
fetch succeeds; when that fetch is unavailable, the address is gone from the
page entirely. The `soroban_invocations` / `soroban_events` light lists are
fallback-only (`[]` whenever the archive fetch succeeds), so they are not the
argument.

**A ClickHouse regression test now guards the contract-detail SQL.**
`crates/api/src/contracts/queries_ch_tests.rs` creates a throwaway database,
applies the real schema with `db_clickhouse::apply_init_sql`, seeds an
own-code contract, a SAC, a fleet member with two targets for its tag, and a
fleet member whose tag has no target, then runs `fetch_contract` and
`fetch_wasm_interface` and asserts every resolved hash, owner, tag and ABI.
Gated on `CH_URL` like the other DB-backed tests in the crate. Proven against
the defect it exists for: with the `max(ledger) AS ledger` alias put back, it
fails with `Code: 184 ... ILLEGAL_AGGREGATION` (exit 101); restored
byte-for-byte, it passes again.

**D5 correction: the transaction page is mostly archive-sourced.**
`get_transaction` runs the DB operations query and the public-archive fetch in
parallel; the archive block (`heavy`) carries each operation's full parser
details, including `"contractId"` as text (`xdr-parser/src/operation.rs`). The
DB appearance lists are only a fallback for when the archive fetch fails. The
one place the address still comes from the DB is the operation headline:
`humanizeOp.ts` reads `functionName` from `heavy` but the address from
`light.contract_id`. Reading `heavy.details.contractId` there instead removes
the transaction page's need for any surrogate-to-StrKey lookup in the normal
case, which takes away the last concrete argument for `contract_addresses`.

**Side finding, not protocol 28: 138 deployed SACs are flagged as not deployed.**
Found while checking whether pool legs could derive their SAC address
(`derive_sac_strkey`, ADR 0051) instead of joining `asset_sac` →
`soroban_contracts`. On production, `asset_sac.sac_deployed` never over-claims
(0 flagged deployed without a deployment row) but under-claims for **138**:
all are `is_sac = true` deployment rows, all deployed between ledgers
58,628,632 and 62,803,627, each with exactly one `asset_sac` row. The
mechanism is visible on `develop` in `persist/stage.rs`: the deployment path
writes the deployed facet only when the deploy's asset identity resolved in the
same batch (`let Some(sac) = &dep.sac_asset else { continue }`), while the
override path for an address that emits events writes `deployed = 0`. Why the
identity failed to resolve in that window, and why it stopped after
62,803,627, is not proven.

Consequence, read from code rather than seen on a live page: the assets API
takes the link from this flag, so those 138 SACs render as unlinked and "not
deployed" on the asset pages. It also means the pool join cannot simply be
swapped for the flag — that would drop the address for the same 138. The
principled direction is the one ADR 0051 already takes for the address itself:
derive, don't copy. Deployment is a fact of `soroban_contracts`, keyed by an id
that is itself derivable from `code:issuer`; the flag is a denormalised copy
that has drifted. Not fixed here; belongs with `0452` (reserved SAC
visibility) or `0503` (the completeness audit).

**D5 decided 2026-09-11: one table (option B).** `contract_addresses` is
removed again; `soroban_contracts` is the only home for contracts, written on a
deployment or an executable update and never on a mere mention. Readers that
resolve a surrogate go back to it. The transaction page's operation headline now
reads the called contract from the archive block (`heavy.details.contractId`)
and keeps the DB value only as the degraded-mode fallback, so an address with no
contract behind it still renders when the archive is reachable.

**D6 is not a new task.** `0503_OPS_exhaustive-completeness-audit-against-network-state`
already owns this defect class: its "In-ledger ordering audit (2026-08-19)"
table verifies the fold at every state-table emit site — and
`soroban_contract_metadata` is simply missing from it. The finding belongs
there as one more row.

**Not deployable until the DDL runs.** Two metadata-only `ALTER`s on
`soroban_contracts` plus the new table, and they must land BEFORE the code that
writes them or the driver rejects the inserts client-side (task 0310). The
columns MUST carry `DEFAULT NULL` — the first version of this paragraph said
"before the code" and nothing about a default, and that caused the incident
below. Done on production 2026-09-11; the statements as they now stand:

```sql
ALTER TABLE soroban_contracts ADD COLUMN IF NOT EXISTS executable_owner_id Nullable(Int64) DEFAULT NULL;
ALTER TABLE soroban_contracts ADD COLUMN IF NOT EXISTS executable_tag Nullable(String) DEFAULT NULL;
-- contract_executable_refs: CREATE TABLE verbatim from init.sql
```

### Incident 2026-09-11 — the column DDL froze ingestion for 31 minutes

**Impact.** No ledger persisted from 15:05:28 to 15:36:49 UTC; the site served
data up to ~31 min stale; ingestion caught up at 15:45. Nothing was lost or
duplicated (verified below). `production-indexer-ch-write-failures` paged at
15:06, `production-ingestion-backlog-age` at 15:12; both back to OK by 15:56.

**Cause.** The two `ADD COLUMN`s ran as `Nullable(...)` with no `DEFAULT`,
ahead of this branch's deploy. The indexer build still running in production
writes `SorobanContractRow` without the new fields, and clickhouse-rs 0.15
validates the row struct against `DESCRIBE TABLE` before every insert: a
table column the struct lacks must have a default (`Default`, `Materialized`
or `Alias` — `row_metadata.rs`), and `Nullable` alone is not one. Every insert
failed client-side with `schema mismatch … the following non-default columns
are missing: executable_owner_id, executable_tag`; the reconcile failed, the
doorbell was redelivered, and the same failure repeated.

**Why the first fix did not unstick it.** `MODIFY COLUMN … DEFAULT NULL` on
both columns (15:22) corrected the table, but the driver caches the
`DESCRIBE` result per client, and the client lives for the whole Lambda
execution environment (`crates/indexer/src/main.rs`). A reported batch-item
failure is not a crash, so Lambda never reset the environment; per the AWS
execution-environment docs only a crash or timeout does, otherwise
environments are recycled "every few hours". A no-op
`aws lambda update-function-configuration --region eu-central-1 --description …`
at 15:36:47 brought a new environment: new `DESCRIBE` at 15:36:51, zero
errors from it, catch-up at ~0.9 ledgers/s against the network's ~0.18
(estimate from ~5.5 s closes), i.e. the lag shrank ~4 s per second.

**Why it was missed.** Task 0310 recorded this driver rule, but framed it
around DROPPING columns ("deploy + ALTER + recycle are one window"); for an
ADD, "column before code" was taken to be safe on its own. The statements were
never checked against the struct running in production, only against this
branch's struct, which names both columns and so passes. `init.sql` had no
default either, so the DDL matched the repository and looked right.

**Verification that nothing was damaged** (production, read-only, 2026-09-11):

| check                                                     | result                                                                                                                                                                                              |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ledgers` 64,379,000 – 64,380,287                         | 1,288 rows = 1,288 distinct = the range: no gap, no duplicate                                                                                                                                       |
| `transactions` in the stall range 64,379,862 – 64,380,287 | 158,204 rows = 158,204 distinct = `sum(transaction_count)` from the ledger headers                                                                                                                  |
| partial writes from the failed attempts                   | no new part in `accounts`, `soroban_contracts`, `transactions` during the stall; first retried ledger holds one row per key (`accounts` 30/30, `account_entry_state` 27/27, `transactions` 343/343) |
| enrichment                                                | publish runs only after a successful persist (`handler/mod.rs`); queue empty                                                                                                                        |
| the DDL itself                                            | metadata-only: 0 mutations; new columns NULL everywhere; new table empty                                                                                                                            |
| DLQ `production-ledger-processor-dlq`                     | 0 — the stall ended before any doorbell reached `maxReceiveCount`                                                                                                                                   |

The recovery was clean for the reason task 0241 designed for: the reconcile
resumes from `max(sequence)` and the `ledgers` row is written last, so a
failed attempt leaves nothing that counts as persisted.

**What changes.** Every `ADD COLUMN` carries an explicit `DEFAULT` (for a
`Nullable`, `DEFAULT NULL`), so "column first, code later" is actually safe;
any `ALTER` on a table the indexer writes is checked against the struct
running in production before it is handed over, and is followed by the
environment recycle above. `init.sql` now declares the default, and the rule
is written into `docs/deployment.md` next to the existing DDL gotcha.

### The stub mechanism itself, measured

Chasing gap 2 turned up something larger, and it is not a protocol-28 problem
at all. Pass-2 writes an FK stub for every contract merely _referenced_ by an
op or event, and the stage is batch-local — it suppresses a stub only for
contracts seen in the same batch, never for ones already in the table. On
production:

|                                                 | rows    |
| ----------------------------------------------- | ------- |
| rows in `soroban_contracts`                     | 186,999 |
| distinct contracts                              | 148,981 |
| stub rows (`wasm_uploaded_at_ledger = 0`)       | 37,950  |
| contracts holding BOTH a real row and stub rows | 35,509  |

So ~37,900 of the 37,950 stub rows carry no information the table does not
already have. RMT resolves them on merge — version 0 loses — but prod RMT
tables are not merged to one part (task 0420), so they persist as duplicates,
which is exactly the read trap this schema documents.

They are not inert either: `fetch_contract_list` selects `WHERE 1` with no
stub filter, so the 54 stub-only rows appear in the public contracts list as
contracts, and `network/queries.rs:103` counts them.

Why the mechanism exists, before anyone deletes it: the surrogate id is
`hash64(strkey)` — **irreversible**. Without a row carrying the text, a stored
id can never be rendered as a `C…` address. And one read path,
`nfts/queries.rs:678`, uses an **INNER JOIN** on `soroban_contracts`, so a
missing row does not blank a column — it drops the NFT row from the response
entirely. "Just stop writing stubs" silently loses data there.

The clean shape is to split the two meanings: a small id→StrKey lookup table
that joins keep using, and `soroban_contracts` holding only contracts actually
observed to exist. Separate task — it is neither caused by nor blocking
protocol 28.

**Checked and clear:** `token_metadata.rs` compares with `==` against
`StellarAsset`, so an external ref correctly falls through as non-SAC.
`op_source.rs` reads the contract-id preimage, not the executable.
`extract_wasm_hash` yields `None` but still writes the deployment row, so no
contract disappears. The frontend never renders an executable type — the string
does not appear outside generated code. `contract_data_balance` and the other
`_ =>` arms in `ledger_value.rs` match balance-key shapes, which an executable
tag can never be.

**Thread 10 — the sibling `prices` repo: yes, affected, and it is worse than a
bump.** `rumblefishdev/stellar-prices-api` on `master` pins
`stellar-xdr = "=27.0.0"` (an exact pin) and takes `xdr-parser` as a git
dependency on THIS repo's `develop` branch, locked at rev `d61b359f`. So:

1. The 26→27 bump that 0368 left open there DID land — they are on 27.0.0. That
   follow-up can be closed as done.
2. They need the same 27→28 bump before the vote, or their decode fails exactly
   as ours would have.
3. Once our bump reaches `develop`, their exact `=27.0.0` and our `^28` cannot
   coexist in one graph. Their lockfile hides this until someone runs
   `cargo update` or rebuilds the lock — then their build stops resolving. The
   two bumps have to be coordinated, not sequenced arbitrarily.

### Deep review of PR #456 (2026-09-14) — two regressions fixed before deploy

A multi-lens review (correctness, simplify, security, devil's advocate,
production data, architecture, pattern generalisation, then an independent
judge) returned REQUEST CHANGES. Verdict and evidence are summarised here; the
two blocking defects are fixed on this branch.

**Fixed — the live upgrade prefetch failed on every call.**
`fetch_prior_contract_rows` still selected the 8 pre-0548 columns into the now
10-field `SorobanContractRow`; clickhouse-rs rejects the length mismatch
client-side, the error was logged as a warning and swallowed, and every
`executable_update` (Wasm and CAP-85 reference alike) was skipped. On
production that is 24 upgrades in the last 7 days (~3.4/day, 7-day average),
with no alarm and no recovery job — `wasm-upgrade-backfill`, which the log line
promised, was removed in task 0425. CI on the PR head was red on
`g9_verdict_routing_e2e` for exactly this; the test passes on the merge base.
Now `SELECT ?fields`, so the struct is the only column list.

**Fixed — `repair-tier1` silently reset columns before its table swap.** Its
staging INSERTs named the columns by hand, so every column added later was
filled from its DEFAULT and the EXCHANGE replaced the live values:
`executable_owner_id` / `executable_tag` (new here), and `lp_positions.closed_at_ledger`
(pre-existing; 779 of 112,385 production positions are closed and the next
repair would have reopened them). Both rebuilds now copy with `* REPLACE (…)`.
Two ClickHouse-backed tests plant the columns, run the real swap and read
them back; both fail against the previous SQL and pass against the new.

**Fixed after an automated PR review (same day).** CAP-85 references were
folded per transaction only, so two re-points of one tag in two transactions
of one ledger tied on version; `build_executable_ref_rows` now folds across
the ledger. The upgrade prefetch now fails closed: its error aborts the ledger
before the commit marker and the reconcile retries, instead of skipping an
upgrade that nothing would ever recover.

Declined from that review, each for a stated reason: resolving the reference
outside the 45-second response cache (every mutable contract field, including
a plain Wasm upgrade, already carries that TTL); a throwaway database per
repair test (the crate's CH tests share one database by convention); giving
the backfill sink a prior-row prefetch and classifying fleet members by the
code they run (both already listed below as follow-ups, not regressions of
this branch).

**Recorded in `docs/deployment.md`:** after a protocol vote the rollback floor
is the first build carrying the matching `stellar-xdr` pin. For this vote that
is commit `840f2b58` — the bump alone, which decodes protocol 28 and inserts
cleanly against the production schema.

**Open from the review, not blocking the vote:**

- A non-UTF-8 tag is stored as the literal `"<invalid-utf8>"` and becomes a
  key; `asset_code.rs` (task 0359) already rejected that policy.
- The contract-detail query joins the refs subquery unconditionally
  (production: ~45 → ~130 MiB per request with an empty table); the two copies
  of that subquery resolve the hash in two different ways.
- The new contract-detail ClickHouse test never runs in CI (it gates on
  `CH_URL`; CI sets `CLICKHOUSE_URL`).
- The testnet fixture test cannot tell stellar-xdr 28 from 27 — the fixture
  decodes with the v26 CLI and carries no protocol-28 arm. The constructed
  round-trip tests are the real gates.
- The placeholder-row cleanup (`ALTER TABLE soroban_contracts DELETE WHERE
wasm_uploaded_at_ledger = 0`) must run after the new indexer is live — the
  running build adds ~2,450 such rows a day — and is not yet an operator step.
- Filtering the transaction list by a never-deployed contract address now
  returns an empty page (a consequence of D5 not recorded there).
- Follow-ups outside this PR: a typed executable enum instead of string-matched
  JSON; `backfill-runner`'s sink applies no executable updates at all (empty
  prior-row map, pre-existing); classification of fleet members by the code
  they run.

## Acceptance Criteria

- [ ] `galexieImageTag` pinned to the Galexie 28.0.1 ECR digest, read back from
      ECR (not copied from Docker Hub)
- [x] ~~GitHub env `GALEXIE_IMAGE_DIGEST` updated~~ — not needed: nothing reads
      it since task 0390 (`docs/deployment.md`, Galexie recipe)
- [ ] Galexie 28.0.1 live in prod, S3 exports flowing, before 2026-09-16 17:00 UTC
- [x] Workspace `stellar-xdr` = 28; `cargo check --workspace --all-targets` green
      (2026-09-10)
- [x] `ContractExecutable::ExternalRef` handled at all render sites, with a test
      per site; no fallback that mimics `wasm`. **Four sites, not three** — the
      fourth is `ScVal::ExecutableTag` in `scval.rs`
- [x] A testnet proto-28 ledger decodes clean — ledger 4,601,991, committed as a
      fixture and asserted to report `protocol_version = 28`. Weaker than it
      reads: the fixture also decodes under stellar-xdr 26 (review 2026-09-14);
      the constructed-arm round-trips are what fail under 27
- [ ] Post-vote: indexer decodes mainnet proto-28 ledgers, DLQ stays empty,
      ingestion-lag alarm quiet
- [ ] **Decision needed** — what `wasm_hash` and the upgradeable chip should say
      for an external-ref contract (gaps 1 and 2 above). Not a Sep-16 blocker;
      bites the first time a mainnet contract uses CAP-85
- [x] Production schema for this branch in place — both `soroban_contracts`
      columns with `DEFAULT NULL`, `contract_executable_refs` created
      (2026-09-11; see the incident above)
- [ ] Sibling `prices` repo bumped in step with this one — its exact `=27.0.0`
      pin cannot coexist with our `^28` once `develop` moves
- [x] **Docs updated** — the expected `N/A` turned out to be wrong. Two
      architecture docs stated that a protocol upgrade is handled by bumping the
      `stellar-xdr` pin, full stop. Our own two incidents disprove that, so both
      now name the Galexie half and the compiler-invisible half:
      `technical-design-general-overview.md`,
      `infrastructure/infrastructure-overview.md`. Schema, endpoints, pipeline
      steps and topology are unchanged — those stay `N/A`
- [x] **API types regenerated** — the pin bump alone left `openapi.json`
      byte-identical; the executable-reference model then added
      `executable_owner` / `executable_tag` to `ContractDetailResponse`,
      regenerated with it

## Future Work

Not auto-created as backlog tasks — pending confirmation:

- **CAP-85 semantics vs `wasm_hash` and the upgradeable badge.** External-ref
  executables mean a contract's code identity lives in another contract. That
  undercuts the stale-`wasm_hash` fix (0320/0326) and the upgradeable badge
  (0327), whose premise — "does this contract import the upgrade host function" —
  is wrong for a fleet member whose mutability sits with the owner. Also gives
  `contract_deployments` a third executable kind with no direct hash. Not a
  Sep-16 blocker: it only bites once mainnet contracts actually use external refs.
- **Sibling `prices` repo.** `prices-production-ledger-processor` builds from a
  separate repo and needs the same stellar-xdr bump. 0368 left the 26→27 bump
  there open as an external follow-up — confirm whether that ever landed before
  assuming 28 is the only gap.
- Carried over, still open from 0367: ledger-advance healthcheck (replace
  `pgrep -x stellar-core`), and persistent BucketList state (EFS) to cut the
  ~20-min restart warmup.

## Notes

- Key dates: Core stable 2026-08-13 · testnet vote 2026-08-27 17:00 UTC ·
  validator arming 2026-09-09 17:00 UTC (not ours) · **mainnet vote 2026-09-16
  17:00 UTC**.
- Source announcement came from the team Slack on 2026-09-08, linking the two SDF
  posts in `links` above.
