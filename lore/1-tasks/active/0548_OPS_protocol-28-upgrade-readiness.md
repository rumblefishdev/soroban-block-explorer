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
(no chip), but telling an external ref apart from a SAC needs a stored marker,
which we do not have. Same decision as gap 1.

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

## Acceptance Criteria

- [ ] `galexieImageTag` pinned to the Galexie 28.0.1 ECR digest, read back from
      ECR (not copied from Docker Hub)
- [ ] GitHub env `GALEXIE_IMAGE_DIGEST` updated (production + staging)
- [ ] Galexie 28.0.1 live in prod, S3 exports flowing, before 2026-09-16 17:00 UTC
- [x] Workspace `stellar-xdr` = 28; `cargo check --workspace --all-targets` green
      (2026-09-10)
- [x] `ContractExecutable::ExternalRef` handled at all render sites, with a test
      per site; no fallback that mimics `wasm`. **Four sites, not three** — the
      fourth is `ScVal::ExecutableTag` in `scval.rs`
- [x] A testnet proto-28 ledger decodes clean — ledger 4,601,991, committed as a
      fixture and asserted to report `protocol_version = 28`
- [ ] Post-vote: indexer decodes mainnet proto-28 ledgers, DLQ stays empty,
      ingestion-lag alarm quiet
- [ ] **Decision needed** — what `wasm_hash` and the upgradeable chip should say
      for an external-ref contract (gaps 1 and 2 above). Not a Sep-16 blocker;
      bites the first time a mainnet contract uses CAP-85
- [ ] Sibling `prices` repo bumped in step with this one — its exact `=27.0.0`
      pin cannot coexist with our `^28` once `develop` moves
- [x] **Docs updated** — the expected `N/A` turned out to be wrong. Two
      architecture docs stated that a protocol upgrade is handled by bumping the
      `stellar-xdr` pin, full stop. Our own two incidents disprove that, so both
      now name the Galexie half and the compiler-invisible half:
      `technical-design-general-overview.md`,
      `infrastructure/infrastructure-overview.md`. Schema, endpoints, pipeline
      steps and topology are unchanged — those stay `N/A`
- [x] **API types regenerated** — ran clean; `openapi.json` came back byte-identical,
      so the bump changes no API surface

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
