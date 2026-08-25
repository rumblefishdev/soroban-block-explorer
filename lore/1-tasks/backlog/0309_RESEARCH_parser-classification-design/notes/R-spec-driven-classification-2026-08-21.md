---
title: 'Research: spec-driven classification and event decoding — what the standards say, what implementations do'
type: research
status: mature
spawned_from: ../README.md
spawns: []
tags:
  [
    parser,
    classifier,
    architecture,
    completeness,
    sep-48,
    sep-50,
    sep-41,
    adoption,
    measured,
  ]
links:
  - 'https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0048.md'
  - 'https://github.com/stellar/stellar-xdr/blob/curr/Stellar-contract-spec.x'
  - 'https://github.com/stellar/rs-soroban-sdk/pull/1473'
  - 'https://github.com/stellar/wallet-backend/blob/main/README.md'
history:
  - date: 2026-08-21
    status: mature
    who: karolkow
    note: 'Primary-source pass on Q1-Q7 (SEP/XDR/SDK/CLI, Soroban + EVM indexers, CAPs). Includes an original mainnet measurement of the 66 silent-drop contracts (SDK version + spec entry kinds) and a production measurement that refutes the "byte-identical transfer" premise.'
---

# Research: spec-driven classification and event decoding

> Question: how SHOULD a Soroban explorer classify contracts and decode token/NFT events?
> Method: primary sources only — SEP texts and their git history, the `stellar-xdr` `.x`
> files, first-party SDK/CLI/indexer source, plus original measurement against mainnet and
> our own production tables. Sibling note `R-classification-design-patterns.md` covered the
> generic pattern literature; this one is the Stellar-specific, version-pinned, measured layer.

## Headline

**The machine-readable answer already exists on chain for 42% of the contracts we silently
drop, and we throw it away in one line of code.**

`crates/xdr-parser/src/contract.rs:148` keeps `ScSpecEntry::FunctionV0` and discards every
other variant. Measured against the 66 contracts that carry a decisive `Nft` verdict and
produce zero rows anywhere (task 0392 F3): **18 declare full SEP-48 event specs** (333
`event_v0` entries between them, giving exact topic/data layout and types), and **10 more
declare their event payloads as `*Event` UDT structs**. That is 28 of 66 decodable today with
no guessing, no heuristics, and no new dependency — we already build against `stellar-xdr`
27.0.0, which has `ScSpecEntry::EventV0`.

The other 38 have nothing event-shaped in their spec, and that is not a mystery either: it is
**explained by the SDK version stamped in their own Wasm** (see F-A). The residual UNKNOWN is
therefore not a black box — it is a bucket with a machine-readable reason code attached to
every member.

Two established premises turned out to be **wrong**, both refuted below with primary sources
or production data: the "byte-identical transfer event" premise (F-C), and the reading of
SEP-50's canonical transfer shape that puts `token_id` in the topics — it is in the **data**
(see _Corrections_).

---

## Original measurement (this session)

Method: the 133 contracts with a decisive `Nft` verdict, deduped per the F6 trap with
`argMax(contract_type, wasm_uploaded_at_ledger)`; the 66 with no row in `nfts` extracted; each
one's Wasm fetched from mainnet through the official CLI (`stellar contract info meta` /
`... interface`, stellar-cli 26.0.0) against a public RPC. Raw output kept in the scratchpad.

### F-A — the absence of event specs is fully explained by SDK version

`#[contractevent]` — the macro that writes a SEP-48 event spec into the Wasm — was added in
**soroban-sdk v23.0.0**, released 2025-09-03 (rc 2025-07-16), in
[PR #1473](https://github.com/stellar/rs-soroban-sdk/pull/1473). The same release
[deprecated](https://github.com/stellar/rs-soroban-sdk/pull/1524) the old
`env.events().publish(...)` path. Every contract compiled before that release therefore
**cannot** carry an event spec, as a matter of build history rather than developer choice.

The SDK stamps its own version into the Wasm automatically, as
[`soroban-sdk/src/lib.rs`](https://github.com/stellar/rs-soroban-sdk/blob/main/soroban-sdk/src/lib.rs)
shows:

```rust
contractmeta!(key = "rsver", val = env!("RUSTC_VERSION"),);
#[cfg(not(soroban_sdk_internal_no_rssdkver_meta))]
contractmeta!(key = "rssdkver", val = concat!(env!("CARGO_PKG_VERSION"), "#", env!("GIT_REVISION")),);
```

Reading `rssdkver` off all 66 and cross-tabulating against actual `event_v0` presence:

| Bucket                                     | Contracts | Declare ≥1 `event_v0` |
| ------------------------------------------ | --------- | --------------------- |
| soroban-sdk **< 23** (macro did not exist) | 30        | **0** — deterministic |
| soroban-sdk **≥ 23** (macro available)     | 35        | **18** (17 opted out) |
| `rssdkver` unreadable                      | 1         | 0                     |

The correlation on the lower half is perfect and causal, not statistical. On the upper half
it is a genuine developer choice: **17 of 35 contracts built on a capable SDK still emit no
event spec**, because `#[contractevent]` is opt-in and the deprecated manual `publish` path
still compiles.

**This is the single most useful operational finding in the note.** `rssdkver` turns "we
could not decode this contract" from an undifferentiated failure into a reason code:
_pre-SDK-23 (impossible)_ vs _SDK-23+ but opted out (possible, author chose not to)_ vs
_declared and we failed anyway (our bug)_. We do not currently parse `contractmetav0` at
ingest at all — the only reader in the tree is an on-demand API decompiler
(`crates/api/src/runtime_enrichment/wasm_code.rs:195`), and it never persists the value.

### F-B — what the 66 actually declare

| What the Wasm spec carries              | Contracts | Decodability                                |
| --------------------------------------- | --------- | ------------------------------------------- |
| SEP-48 `event_v0` entries (333 total)   | **18**    | exact — topics, data, types, format         |
| No `event_v0`, but `*Event` UDT structs | **10**    | field names + types, topic mapping inferred |
| Nothing event-shaped                    | 38        | needs another route                         |

Worked example — `CBT5JMDOUAU3BJF7YZR42LVODLMZSQE4LIJUJNUBKEC2VZOXIF4JFBRU`, one of the 66,
and one of task 0392's two hand-inspected F4 contracts. Its spec has 31 entries: 19
`function_v0`, **10 `udt_struct_v0`**, 1 `udt_union_v0`, 1 `udt_error_enum_v0`. We keep 19 and
drop 12 — 39% of the spec. Among the dropped:

```
MintedEvent  -> badge_type: symbol, cycle_id: u32, token_id: u32, wallet: address
```

That is precisely the `minted` event that produces zero rows. Its shape was on chain the
whole time. Its `rssdkver` is **22.0.11** — below 23, so the absent event spec is explained,
not anomalous.

### F-C — REFUTED: NFT and fungible `transfer` events are _not_ byte-identical

The task README states, as a constraint on the design:

> "`Fungible` and NFT `transfer` events are byte-identical in shape (`from,to,i128` vs
> `from,to,token_id`); only WASM classification separates them."

The sibling research note also records "i128 = fungible" as refuted 0-3, on the grounds that
"real NFTs use i128". **Both are wrong.** Measured over our own `soroban_events`, grouping
`transfer` events by the `type` tag our decoder already preserves in `data_xdr`:

| Verdict of the emitting contract | `transfer` data SCVal type | Events | Contracts |
| -------------------------------- | -------------------------- | ------ | --------- |
| `Nft`                            | `u32`                      | 8,487  | 17        |
| `Nft`                            | `map`                      | 71     | 6         |
| `Nft`                            | `vec`                      | 3      | 1         |
| `Fungible`                       | `i128`                     | 286    | 44        |
| `Fungible`                       | `address`                  | 2,034  | 11        |

**Zero overlap.** Not one NFT-verdict contract emits an `i128` transfer payload; not one
fungible-verdict contract emits `u32`/`map`/`vec`. The SCVal type tag is a clean discriminator
on the live data, and it is available on the event alone, with no Wasm fetch and no classifier
round trip.

This matches the standards rather than contradicting them. SEP-50 says `TokenID` "should be an
unsigned integer"
([sep-0050.md](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0050.md));
SEP-41 amounts are `i128`. `SCV_U32` and `SCV_I128` are distinct XDR union arms, so
"byte-identical" was never possible for a spec-following pair.

Caveats, stated honestly: this is a _measurement of contracts we already classified_, so it
cannot prove that no future NFT will use `i128`. It proves the premise blocking the current
design does not hold on today's data. Treat the type tag as a strong prior with an explicit
tie-break, not a proof. The 11 fungible-verdict contracts emitting `transfer` with **`address`**
data are their own signal — that is not SEP-41-shaped either, and suggests the name-based
classifier mislabels in both directions.

---

## Q1 — SEP-48 event specs in depth

**Status and dates.** SEP-48 _Contract Interface Specification_ is **Active**, Version 1.1.0,
Created 2025-03-26. Its own preamble says `Updated: 2025-04-16`, but that header is **stale**:
the event-spec section was merged 2025-07-31 in
[PR #1766](https://github.com/stellar/stellar-protocol/commit/a7c895b2) ("SEP-48: Add events").
So the event machinery is roughly **13 months old** as of this note.

**XDR.** Verified two ways — the canonical
[`Stellar-contract-spec.x`](https://github.com/stellar/stellar-xdr/blob/curr/Stellar-contract-spec.x)
and the generated Rust in the crate we already vendor (`stellar-xdr` **27.0.0**,
`src/generated/sc_spec_event_v0.rs`). They agree:

```xdr
struct SCSpecEventV0 {
    string doc<SC_SPEC_DOC_LIMIT>;
    string lib<80>;
    SCSymbol name;
    SCSymbol prefixTopics<2>;
    SCSpecEventParamV0 params<>;
    SCSpecEventDataFormat dataFormat;
};
struct SCSpecEventParamV0 {
    string doc<SC_SPEC_DOC_LIMIT>;
    string name<30>;
    SCSpecTypeDef type;
    SCSpecEventParamLocationV0 location;
};
enum SCSpecEventParamLocationV0 { SC_SPEC_EVENT_PARAM_LOCATION_DATA = 0,
                                  SC_SPEC_EVENT_PARAM_LOCATION_TOPIC_LIST = 1 };
enum SCSpecEventDataFormat { SC_SPEC_EVENT_DATA_FORMAT_SINGLE_VALUE = 0,
                             SC_SPEC_EVENT_DATA_FORMAT_VEC = 1,
                             SC_SPEC_EVENT_DATA_FORMAT_MAP = 2 };
```

- `location` has exactly **two** values: `DATA` and `TOPIC_LIST`.
- `dataFormat` has exactly **three**: `SINGLE_VALUE`, `VEC`, `MAP`.
- `prefixTopics` is capped at **2 elements** — the static discriminator budget is two symbols,
  no more.
- **Spec-vs-XDR divergence:** SEP-48's prose writes `SCSpecEventParamV0 params<50>;` (twice,
  at lines 307 and 897), while the actual XDR and the generated Rust have `params<>`
  (unbounded). Trust the `.x` file; do not hard-code 50 as a validation limit.

`ScSpecEntry::EventV0` **is present in both `stellar-xdr` 26.0.1 and 27.0.0**, and our
workspace pins `stellar-xdr = { version = "27" }`. Reading event specs needs no dependency bump.

**How matching works, verbatim from SEP-48:**

> "Event parsers should use the static values to distinguish one type of event from another,
> and to match on events when filtering or mapping raw events to their specified equivalents.
> When matching, parsers should tolerate static topics being of the `SCVal` type `SCV_SYMBOL`
> or `SCV_STRING` because some contracts have emitted their topics as strings."

and on the non-uniqueness that the sibling note flagged:

> "Event parsers can use the types of parameters to distinguish one type of event from another
> in the case where events share the same prefix topics, as is the case in some contract
> interfaces, e.g. [SEP-41]."

So the normative algorithm is a **two-stage match**: prefix-topic equality (symbol-or-string
tolerant), then typed-parameter tie-break. Parameters with `location = TOPIC_LIST` appear
**after** the prefix topics; parameters with `location = DATA` are packed per `dataFormat`.
The reference implementation is the official CLI's
[`soroban-spec-tools/src/event.rs`](https://github.com/stellar/stellar-cli/blob/main/cmd/crates/soroban-spec-tools/src/event.rs)
(`match_event_to_spec` → `matches_prefix_topics`), driven from
[`commands/events.rs`](https://github.com/stellar/stellar-cli/blob/main/cmd/soroban-cli/src/commands/events.rs),
which builds a per-contract spec cache and tolerates a `None` fetch. That is a working model
for exactly our problem and worth reading before writing our own.

**Storage.** One `contractspecv0` Wasm custom section. The CLI reads the three sections in
[`soroban-spec-tools/src/contract.rs:58`](https://github.com/stellar/stellar-cli/blob/main/cmd/crates/soroban-spec-tools/src/contract.rs):
`contractenvmetav0`, `contractmetav0`, `contractspecv0`. We read only the third.

**Which SDKs emit specs, and is it optional?**

| Fact                                   | Value                                                                                                                                            |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `#[contractevent]` introduced          | soroban-sdk **v23.0.0**, 2025-09-03 ([PR #1473](https://github.com/stellar/rs-soroban-sdk/pull/1473))                                            |
| `env.events().publish()` deprecated    | same release ([PR #1524](https://github.com/stellar/rs-soroban-sdk/pull/1524))                                                                   |
| Spec emission toggleable on the macro? | **No** — no argument disables it ([docs.rs, soroban-sdk 27.0.6](https://docs.rs/soroban-sdk/latest/soroban_sdk/attr.contractevent.html))         |
| Emitting an event spec at all          | **Optional in practice** — using the deprecated `publish` path yields none                                                                       |
| `data_format` default                  | `map`                                                                                                                                            |
| Default prefix topic                   | `snake_case(StructName)` — an SDK convention, and SEP-48 says so explicitly: "This is not a feature of this proposal, but a feature of the SDK." |

Two caveats that bite a spec-driven decoder:

- **Spec shaking.** The SDK emits `contractmeta!(key = "rssdk_spec_shaking", val = "2")` and,
  under `experimental_spec_shaking_v2`, "markers are embedded that allow post-build tools to
  strip spec entries for events that are never published at a contract boundary". A spec entry
  can therefore be _removed after compilation_. Whether shaking can strip an event that **is**
  reachable is **UNVERIFIED**.
- **`#[contracttrait]`'s `spec_export` defaults to `false`**
  ([docs.rs](https://docs.rs/soroban-sdk/latest/soroban_sdk/attr.contracttrait.html)), so spec
  entries for non-overridden default trait methods may be absent. A deployed contract's spec
  is a **lower bound** on its interface, never an upper bound.

---

## Q2 — Is there an authoritative way to know what standard a contract implements?

**No, and there is no ERC-165 analogue, proposed or otherwise.**

A search of `stellar/stellar-protocol` issues and PRs for `supportsInterface` OR `ERC-165`
returns **zero results**. There is no runtime interface-detection call in Soroban, and none is
proposed. The architectural reason is recorded in the sibling note: stream indexers cannot
invoke contracts, so Stellar chose static Wasm metadata over a runtime call.

The nearest mechanism is **SEP-47 Contract Interface Discovery**, and it has **not progressed**:

|                         | SEP-47         | SEP-48     | SEP-50     | SEP-46     | SEP-41     |
| ----------------------- | -------------- | ---------- | ---------- | ---------- | ---------- |
| Status                  | **Draft**      | **Active** | **Draft**  | **Active** | **Draft**  |
| Version                 | 0.1.0          | 1.1.0      | 0.1.0      | 1.0.0      | 0.5.1      |
| Created                 | 2025-02-14     | 2025-03-26 | 2025-03-10 | 2025-02-13 | 2023-09-22 |
| Last substantive commit | **2025-04-16** | 2025-07-31 | 2025-04-08 | 2025-04-16 | 2026-08-03 |

SEP-47's entire mechanism is one meta key — `contractmeta!(key="sep", val="41,40")` — and the
spec disclaims itself:

> "Contracts may claim to implement SEPs that they do not actually implement."
> "No SEP claim ever replaces the need for contract audits and other security measures."

Verification is explicitly out of scope. Its only commits since creation are a rename and two
repo-wide formatting/process sweeps; there are **no open PRs** against it. **Adoption is
effectively zero** — neither of the two contracts I inspected by hand carries a `sep` key, and
the de-facto reference NFT library emits no `contractmeta!` **at all** (zero hits repo-wide),
so no contract built on it can declare conformance even in principle.

Note also that SEP-48 disclaims this role deliberately: it "does not enable contracts to claim
implementation of named interfaces or standards", deferring to SEP-47.

**Conclusion:** self-declaration is unavailable in practice and untrustworthy in principle.
Classification must be **derived** from the spec's structure (function set, event set, typed
shapes), never read off a claim.

---

## Q3 — What other Soroban indexers actually do

Short version: **nobody classifies NFTs, and silent-drop is the ecosystem norm — including in
SDF's own processor.** Only two classification mechanisms exist anywhere, both fungible-only:
SAC identity (re-derive the contract ID from the asset and compare) and SEP-41 exact signature
match.

| System                                                                  | Method                                               | Spec-driven?                                  | NFT concept?                       |
| ----------------------------------------------------------------------- | ---------------------------------------------------- | --------------------------------------------- | ---------------------------------- |
| **stellar.expert**                                                      | SAC (`asset`) vs Wasm only                           | parses SEP-48 in the **browser**, for display | **No** — zero `nft` hits repo-wide |
| **stellar-etl / go-stellar-sdk**                                        | topic[0] symbol + arity + `GetI128()`                | No — `ScSpecEntry` has zero consumers         | No                                 |
| **Hubble / stellar-dbt-public**                                         | none                                                 | Impossible — warehouse holds no Wasm          | No                                 |
| **stellar-rpc**                                                         | n/a — 12 methods, no spec endpoint                   | n/a                                           | n/a                                |
| **stellar-cli**                                                         | **`match_event_to_spec` — full SEP-48 event decode** | **Yes**                                       | No                                 |
| **wallet-backend** (SDF)                                                | `matchSEP41Spec` over `ScSpecEntry`, per Wasm hash   | **Yes**                                       | **Explicitly untracked**           |
| **js-stellar-sdk**                                                      | `event_spec.ts` reads `scSpecEntryEventV0`           | Yes (library only)                            | No                                 |
| **Goldsky** (Turbo only) / **SubQuery** / **Blockdaemon** / **Alchemy** | XDR→JSON passthrough; you decode                     | No                                            | No                                 |
| **Mercury**                                                             | author instruments their own contract and redeploys  | No                                            | No                                 |
| **Subsquid / SQD**                                                      | **no Stellar support at all**                        | —                                             | —                                  |

Three things worth lifting:

**1. SDF's own pipeline has our exact defect.**
[`processors/token_transfer/contract_events.go`](https://github.com/stellar/go-stellar-sdk/blob/master/processors/token_transfer/contract_events.go)
requires `topics[0] ∈ {transfer, mint, burn, clawback}` and then:

```go
amt, ok := value.GetI128()
if !ok { return nil, errNotSep41TokenFromMsg("invalid event amount") }
```

An NFT transfer carrying `u32` dies there — and the failure is discarded uncounted:

```go
// You dont bail on error here, since error here means that it is not a sep-41 compliant token event.
if err == nil { events = append(events, ev) }
```

Our acceptance criterion "counted and visible, not dropped silently" is therefore **ahead of
the reference implementation**, not catching up to it. Worth saying so in the task.

**2. `UNKNOWN` as a stored, named state has first-party precedent.** SDF's
[wallet-backend README](https://github.com/stellar/wallet-backend/blob/main/README.md) states
the taxonomy outright — SAC / SEP-41 / **Unknown**, with NFTs named as the untracked Unknown
bucket — and its contract-type enum is `NATIVE | SAC | UNKNOWN`
([`types.go:205`](https://github.com/stellar/wallet-backend/blob/main/internal/indexer/types/types.go)).
UNKNOWN is a value, not an absence. Its `ProtocolValidator` registry
([`validator_registry.go`](https://github.com/stellar/wallet-backend/blob/main/internal/services/validator_registry.go),
extension guide in `docs/data-migrations/adding-a-protocol.md`) is a clean model for adding an
NFT validator beside a fungible one, and it matches **per Wasm hash, not per contract** — which
gives deduplication for free.

**3. The ecosystem splits into two camps, and nobody has built the third.**
_Raw-passthrough_ (stellar.expert, Goldsky, SubQuery, Blockdaemon, Alchemy, Hubble) never drops
an event and never assigns a type — safe, but every consumer re-solves classification.
_Typed-but-narrow_ (the Go processor → stellar-etl → Hubble chain, and wallet-backend) assigns
types but covers only SAC and SEP-41, discarding the rest silently. **We are in camp 2 and have
inherited its defect.** Nobody has built camp 3 — typed _and_ total — even though both building
blocks are shipped production code: the CLI's per-contract spec cache with `match_event_to_spec`,
and wallet-backend's Wasm-hash-keyed validator registry with a named `UNKNOWN`. Combining them is
the whole design.

**4. stellar.expert stores raw and classifies never.** It keeps the full `topicsXdr`/`bodyXdr`
for every event including the ones we drop, and simply declines to say what they are. That is
the mirror image of our failure: they lose nothing and know nothing; we know something and lose
things. Their `validation` field is reproducible-build attestation, **not** typing.

---

## Q4 — OpenZeppelin `stellar-contracts`

The module is at **`packages/tokens/src/non_fungible/`** (crate `stellar-tokens`), not
`contracts/tokens/`. Latest published crate **0.7.2, 2026-06-09**; HEAD builds against
soroban-sdk 27.0.2. There is **no CHANGELOG** in the repo, and the workspace version string is
stale relative to tags — pin by tag, not by `Cargo.toml`.

**It is the de-facto standard by default, not by ratification.** Official Stellar docs route
users to it ([tools/openzeppelin-contracts](https://developers.stellar.org/docs/tools/openzeppelin-contracts),
and [example-contracts/tokens](https://developers.stellar.org/docs/build/smart-contracts/example-contracts/tokens):
"the fastest path is the audited OpenZeppelin Stellar Contracts library"), and
[stellar/soroban-examples](https://github.com/stellar/soroban-examples) has **no NFT example**
at all. But SEP-50 is authored by the same organisation that writes the library, so the spec
does not independently bless it; and `stellar contract init` does not scaffold it.

**Events.** Nine, all `#[contractevent]`, all with default `data_format` → **MAP**:

| Event                                                                | Topics                                      | Data (map)                                    |
| -------------------------------------------------------------------- | ------------------------------------------- | --------------------------------------------- |
| `transfer`                                                           | `"transfer", from:Address, to:Address`      | `{token_id: u32}`                             |
| `mint`                                                               | `"mint", to:Address`                        | `{token_id: u32}`                             |
| `burn`                                                               | `"burn", from:Address`                      | `{token_id: u32}`                             |
| `approve`                                                            | `"approve", approver:Address, token_id:u32` | `{approved: Address, live_until_ledger: u32}` |
| `approve_for_all`                                                    | `"approve_for_all", owner:Address`          | `{operator: Address, live_until_ledger: u32}` |
| `consecutive_mint`                                                   | `"consecutive_mint", to:Address`            | `{from_token_id: u32, to_token_id: u32}`      |
| `set_default_royalty` / `set_token_royalty` / `remove_token_royalty` | royalty extension                           | `{basis_points: u32}` / `{token_id: u32}`     |

**Three findings that directly shape our parser:**

1. **Two wire formats are live for the same library.** The migration from
   `env.events().publish(topics, token_id)` to `#[contractevent]` landed in **v0.5.0
   (2025-10-28)**. Before it, `transfer` data was a **bare `u32`**; after it, a
   **`Map{token_id: u32}`**. Topics are unchanged across the split. Both generations are on
   chain, both are "OpenZeppelin", and a positional decoder will handle one and silently drop
   the other. Our F-C measurement sees exactly this: `u32` (17 contracts) **and** `map` (6).
2. **The fungible module opted out of the map; the non-fungible module did not.**
   `fungible/mod.rs` carries the repo's only `#[contractevent]` arguments —
   `data_format = "single-value"` to preserve SEP-41's bare `i128`, and `topics = ["transfer"]`
   so the muxed variant shares the symbol. The NFT module received neither. So SEP-50's
   documented bare-`TokenID` data no longer matches the reference implementation, and the SEP
   (frozen at 0.1.0, 2025-03-10) was never updated. **Where the SEP and the library disagree,
   the library is what is on chain.**
3. **`consecutive_mint` is a range event.** `batch_mint` emits one event covering
   `from_token_id..=to_token_id` and **no per-token `mint` or `transfer`**. Any ownership index
   built only on `transfer`/`mint` under-counts an entire batch. This is a distinct
   completeness gap from the bespoke-ABI one and is not currently on the task's radar.

**Conformance declaration: none.** No `contractmeta!` anywhere in the repository, hence no
SEP-47 `sep` key. Identity metadata (`name`, `symbol`, `base_uri`) lives in **instance
storage** under `NFTStorageKey::Metadata`, not in meta. The 11-function SEP-50 trait matches
1:1 (arg-name cosmetics aside), so the **function set remains the only reliable identifier**
for this family.

---

## Q5 — "Never silently miss": cross-ecosystem precedent

### What the EVM standards guarantee — less than people assume

**ERC-165** is `Final`, created 2018-01-23. An interface ID is _"the XOR of all function
selectors in the interface"_ — **selectors only**. The spec says outright that this is _"a
subset of Solidity's concept of interfaces … which also defines return types, mutability and
**events**"_. So a positive answer attests that four-byte selectors exist; it says nothing about
behaviour, and **events are outside the interface ID entirely** — which is exactly the wrong
shape for an event-driven indexer.

The trust caveat everyone attributes to ERC-165 is **not in ERC-165**: the document has no
Security Considerations section, and its whole Rationale is two sentences. The canonical
statement lives in **ERC-5269** (`Review`, created 2022-07-15), which back-attributes it:

> "Similar to ERC-165 callers of the interface MUST assume the smart contract declaring they
> support such ERC interfaces **doesn't necessarily correctly support them**."

ERC-5269 also names a structural gap that is directly our situation: ERC-165 _"requires at least
one method to exist in the first place"_ — **a standard expressed purely as event shapes is
undetectable by it.**

| Standard | Status / created  | Requires 165?                  | Wording                                                                                             |
| -------- | ----------------- | ------------------------------ | --------------------------------------------------------------------------------------------------- |
| ERC-721  | Final, 2018-01-24 | yes                            | bold lowercase _"must implement the `ERC721` and `ERC165` interfaces"_ — **not** RFC-2119 uppercase |
| ERC-1155 | Final, 2018-06-17 | yes                            | uppercase **MUST**, `0xd9b67a26`                                                                    |
| ERC-20   | Final, 2015-11-19 | **no `requires` field at all** | zero detection mechanism of any kind                                                                |

ERC-20 goes further and forbids the very heuristic that name-sniffing classifiers use — on
`name`, `symbol` and `decimals` alike: _"OPTIONAL … but interfaces and other contracts **MUST
NOT expect these values to be present**."_

**The collision that makes this our exact problem.** ERC-20's and ERC-721's `Transfer` produce a
**byte-identical `topic0`** — `0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef`
— because `indexed` does not enter the signature string. The only on-log discriminator is
**indexed arity**: 3 topics vs 4. The entire EVM indexing ecosystem lives with this. Our F-C
situation is the same shape and, as it happens, **better served**: our SCVal data type tag is a
cleaner discriminator than arity, and we measured it as perfectly separating today.

ERC-1155 is the one standard that reasons about indexers explicitly, and it treats **events, not
interface declaration, as the load-bearing contract**: _"The ERC-1155 standard guarantees that
event logs emitted by the smart contract will provide enough data to create an accurate record
of all current token balances"_, enumeration _"must be done using event logs"_, and in hybrid
mode _"the ERC-1155 transfer events MUST still be emitted"_.

### What implementations actually do with the residue

| System              | Method                                                      | Unknown retained?                                                         |
| ------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------- |
| **Alchemy NFT API** | ERC-165 **then** observed-transfer fallback                 | **Yes** — closed enum incl. `NO_SUPPORTED_NFT_STANDARD`, `NOT_A_CONTRACT` |
| **Blockscout**      | topic0 + indexed arity. **Never calls `supportsInterface`** | **No** — logged and dropped                                               |
| Etherscan           | undocumented; separate endpoint per standard                | No — open string, no sentinel                                             |
| Moralis / Pinax     | undocumented                                                | **No — structurally unrepresentable**                                     |
| The Graph / SQD     | developer-declared ABI per handler                          | N/A by construction                                                       |
| Dune                | closed pre-dbt decoder + manual overrides                   | No                                                                        |

**Alchemy is the one to copy**, and its own words are the best available articulation of the
goal our task states:

> "We first query `supportsInterface` and then query `hasTransfers`."
> "…we have two token types: `NotAContract` and `NoSupportedNFTStandard`. These token types
> communicate that **we are, in fact, aware of the contract** and we are telling you that it is
> not an NFT contract."

Two structural choices there are worth more than the enum itself: the unknown verdict lives in
**the same field** as the positive verdicts (a consumer reading only the happy path cannot drop
it), and **"cannot classify" and "classified but suspect" are separate axes** (`tokenType` vs
`spamInfo`). Note also the two-tier evidence model — _declared_ capability first, _observed_
behaviour as fallback. Our classifier currently has only the declared tier.

**Blockscout is the cautionary tale, because it is our current architecture.** It classifies by
pattern-matching topic arity, and on no match:

```elixir
rescue
  e in [FunctionClauseError, MatchError] ->
    Logger.error(fn -> ["Unknown token transfer format: #{inspect(log)}", ...] end)
    acc
```

A log line, and the accumulator returned unchanged — no row, no counter. Its `tokens.type` is
`null: false` and its public API enum is closed, so an unclassifiable contract **cannot be
represented** and can only be absent. Most instructive of all: it _does_ have a
`token_instances.error` column, and that column _is_ indexed — but the serializer never emits
it, so a consumer sees `"metadata": null` and cannot tell "fetch failed" from "no metadata".
**That is our acceptance criterion — "a NULL `signature` must not be indistinguishable from
'no event'" — failing in a mature open-source explorer.** If we add an unknown state and do not
carry it through the API contract, we will have rebuilt this bug.

**Pre-standard contracts are handled by hand-maintained allow-lists, everywhere.** ERC-721's own
Backwards Compatibility section concedes CryptoKitties and CryptoPunks; ERC-6551 (`Review`)
explicitly _"does not require the registry to perform an ERC-165 interface check … to maximize
compatibility with non-fungible token contracts that pre-date the ERC-721 standard"_. Alchemy
states it keeps such a list and does not publish its membership. There is no automated solution
in the published record — which is a useful expectation-setter for our own tail of 38.

## Q6 — Ledger state vs events as the source of ownership truth

**Verdict: events are the spine; state is a targeted oracle that can only ever answer "now".**

- **CAP-46-05 restricts contracts to point access.** Verbatim: _"Contract data IO is restricted
  to so-called 'point access' to specific keys. In particular there is no support for 'range
  queries', upper or lower bounds, or any sort of iteration over the keyspace."_ Four storage
  host functions exist; **no iterator**. Important scope note: this constrains _contracts and
  transaction footprints_, not an external reader — an indexer consuming ledger-close metadata
  sees whole `LedgerEntry` values via `LedgerEntryChange` and can build its own `ContractData`
  index. The protocol does not forbid us enumerating; it forbids _contracts_ enumerating.
- **A specific key is readable** via `getLedgerEntries`, which the RPC docs call _"a primary
  method to access your contract data which may not be available via events or
  simulateTransaction"_. Hard limits, from source: **200 keys per call**
  (`getLedgerEntriesMaxKeys`), **latest ledger only** (`atLedger: 0` → "use the latest
  ledger"), TTL keys rejected outright. **There is no "at ledger N" parameter.**
- **Neither SEP-50 nor SEP-41 defines a storage layout.** Both standardise functions and events
  only; a grep of SEP-50 for storage/ledger-key returns nothing normative. So state-reading is
  per-contract reverse engineering by construction.
- **But contracts self-declare their key schema anyway, and it is in the spec we discard.** Both
  F4 contracts ship a `DataKey` union in `contractspecv0`:
  `CBT5JMDO…` → `TokenOwner(u32)`, `TokenBadgeType(u32)`, `Claimed(symbol,u32,address)`,
  `NextTokenId`; `CB2SIYGH…` → `Owner(string)`, `TotalSupply`, `Admin`, `Registrar`, …
  Because `#[contracttype]` enum encoding is specified, that yields a constructible
  `LedgerKey::ContractData{ key: ScVal::Vec([Symbol("Owner"), <id>]) }`. The OpenZeppelin
  library's `NFTStorageKey { Owner(u32), Balance(Address), Approval(u32), ApprovalForAll(..),
Metadata }` is the same thing for that family. **This is a convention, not a standard** —
  the union named `DataKey` is not guaranteed to be the storage key type, and names can change
  between versions. Use it as a verification oracle, never as the ledger of record.
- **Archival makes absence ambiguous — the same failure mode we are trying to remove.** Per
  CAP-46-12, `TEMPORARY` entries die permanently; `PERSISTENT` entries are archived and
  restorable. An archived-but-still-hot entry reads back with `liveUntilLedgerSeq: 0`; after
  CAP-0057 snapshotting, validators keep only a Merkle root and availability depends on whether
  your RPC operator holds that snapshot. A `NULL` from `getLedgerEntries` conflates _never
  existed_ / _temporary and dead_ / _archived and unavailable_. **If we read state, those three
  outcomes must be recorded distinctly.**
- **`simulateTransaction` on `owner_of` is legitimate but "now"-only.** No ledger parameter
  exists; the RPC always passes `latestLedger`. On an archived entry it returns a
  `restorePreamble` rather than the value — you learn "this is archived", not who owns it.
  Restoration is a _transaction_ mechanism, costs rent, and is unavailable to a read-only
  indexer. **SDF runs no public mainnet RPC**; limits are per-provider and undocumented
  centrally (**UNVERIFIED**).
- **RPC retains ~7 days** (`history-retention-window`, default 120,960 ledgers), and the docs
  state plainly that RPC is not "an indexer for historical data" and that events should be
  ingested into your own database. Historical ownership can only come from our own ledger-meta
  ingestion.
- **There is no `getContractSpec` RPC.** The 12 methods contain nothing spec-related. The CLI
  does it in two hops — instance → `wasm_hash` → `getLedgerEntries(ContractCode)` → parse the
  custom sections locally, verifying `sha256(code) == hash`. That is the route for all 66
  contracts, 200 keys per round trip.

---

## Q7 — Recommended architecture

Concrete and opinionated, in the order I would build it.

### 1. Decoding SHOULD be driven by the declared event spec — as the first tier of a cascade, not the only one

Stop discarding non-`FunctionV0` entries. Keep `EventV0` (18 of 66 contracts, immediately),
`UdtStructV0`, `UdtUnionV0`, `UdtEnumV0`. Zero dependency work — `stellar-xdr` 27 already has
them.

Implement SEP-48's normative two-stage match, following
`soroban-spec-tools/src/event.rs`: prefix-topic equality **tolerating `SCV_SYMBOL` or
`SCV_STRING`**, then typed-parameter tie-break for the shared-prefix case. Cache the compiled
spec **per Wasm hash, not per contract** — wallet-backend's choice, and it deduplicates the
133-contract population down to far fewer distinct binaries.

Also fix an adjacent silent-miss while in there: `parse_spec_entries` does `Err(_) => break`,
so one undecodable entry discards **every entry after it**, functions included. A future
`SCSpecEntryKind = 6` would silently truncate specs we currently parse fine. Skip-and-count,
don't break.

### 2. Contracts that declare nothing get a cascade, then a reason code — never a drop

```
L0  SEP-48 event spec            → exact decode                    (18/66 today)
L1  UDT struct / DataKey union   → field-name + type decode        (+10/66)
L2  self-describing MAP payload  → read by map key name, no spec needed
L3  typed-shape discriminator    → SCVal type tag (F-C), topic arity
L4  UNKNOWN, with a reason code  → counted, alarmed, promotable
```

L2 deserves emphasis: since SDK v23 the **default `data_format` is `map`**, so a growing share
of events carry their own field names on the wire. `CBT5JMDO…`'s `minted` payload is
`{badge_type, cycle_id, token_id, wallet}` — decodable with **no spec at all**, just by reading
the map keys. And SEP-41 makes this normative for consumers:

> "The map data format encodes the event data as a `Map` with `Symbol` keys, and **may contain
> additional keys beyond those defined in this SEP. Event consumers must be able to handle keys
> they do not recognize**, and are expected to support both formats."

Open [PR #1994](https://github.com/stellar/stellar-protocol/pull/1994) (2026-08-03) extends the
same permission to **extra topics**. **Exact-shape matching is therefore non-conformant by
spec, not merely brittle.** Any rewrite must decode by name and tolerate unknown keys and
trailing topics — that alone changes the parser's contract.

The reason code on L4 is the part that makes UNKNOWN honest, and F-A shows it is cheap: parse
`contractmetav0` at ingest and store `rssdkver`. Then every unresolved contract carries
`pre_sdk_23` / `sdk_capable_but_undeclared` / `declared_but_undecoded`. The third bucket is our
bug and should alarm; the first is a permanent property of that binary and should not.

### 3. Both a state enum on the row AND a detail table — and a permanence flag

I started this section intending to argue "flag, not table". The strongest available precedent
says **both**, and it is right.

`graph-node` — the most battle-tested indexer with a public schema — carries four layers at
once:

1. **A detail table**, `subgraphs.subgraph_error`: one row per failure with `message`,
   `block_number`, `block_hash`, `handler`, and a validity `block_range`.
2. **An indexed state enum on the main row**: `health enum ('failed','healthy','unhealthy')`,
   plus `fatal_error` and `non_fatal_errors[]`, with
   `create index attr_subgraph_deployment_health`. "How much is degraded" is one cheap query.
3. **A permanence flag**: `alter table subgraphs.subgraph_error add column deterministic boolean
not null default false`, documented in `graph/src/data/subgraph/schema.rs` as
   `// `true`if we are certain the error is deterministic. If in doubt, this is`false`.`
   The semantics are explicit — _Deterministic_: stop, reprocessing will not help;
   _NonDeterministic_: retry with backoff.
4. **Fail-closed reads**: the `subgraphError` query argument defaults to `deny`, and a subgraph
   that wants to tolerate errors must **declare `nonFatalErrors` at deploy time or the
   deployment is rejected**.

That fourth point deserves emphasis, because it is the strongest available answer to the
read-time-filter question this task already litigated: degraded data is **not served by
default**, and opting in is explicit and declared. That is a different thing from a read-time
visibility filter over a polluted table, and it is the shape our own review was reaching for
when it rejected one.

**The `deterministic` flag reframes 0392's F1 result, and this is the most useful single idea in
the note.** Our quarantine is full of contracts whose verdict is already computed and already
not decisive — _deterministic_ failures. The drain mechanism gap 3 describes is a
_non-deterministic_ remedy: wait, re-check, promote. **That mismatch is precisely why the
reconcile would move zero rows**, and a `deterministic` column would have made it visible up
front rather than after a re-measurement. F1's bucket A/B/C split is exactly this distinction,
discovered independently and without a place to record it.

So, concretely, for `soroban_contracts` and the NFT tables:

- **An indexed classification-state enum on the contract row**, carrying the unknown verdicts as
  _values_, not absences — Alchemy's `NO_SUPPORTED_NFT_STANDARD` / `NOT_A_CONTRACT` pattern, and
  wallet-backend's `UNKNOWN`. Prefer an open-domain or freely-widenable set: Pinax's two-value
  closed enum is exactly why its unknowns are unrepresentable, and we need to distinguish at
  least `pre_sdk_23` / `sdk_capable_but_undeclared` / `parser_shape_mismatch` /
  `declared_but_undecoded` (F-A gives us the first two for free).
- **A detail table recording each drop** — ledger, contract, event signature, reason — so
  "counted and visible" is a `GROUP BY`, and so a NULL `signature` is distinguishable from "no
  event".
- **The state must reach the API.** Blockscout has the detail column, indexed, and never
  serialises it; that is how a correct schema still ships the defect.

On `nfts_pending` specifically: nothing here says a quarantine table is wrong in principle —
graph-node has one. What is wrong today is having the **table without the state**, so "absent",
"not an NFT", and "unclassifiable" are indistinguishable, and the 66 contracts of F3 fall into a
fourth category no table represents. Adding the enum is the incremental move; whether
`nfts_pending` then survives is a follow-on question, and it becomes much less load-bearing once
membership is a value. That is a narrower change than superseding **ADR 0046**, and it should be
attempted first — though if the enum makes the table vestigial, retiring it needs a deliberate
replacement ADR (note: **id 0053 is taken**).

### 4. Classification is a set, not a scalar

The current classifier is name-only with `if` precedence, so a contract implementing both
interfaces silently becomes `Nft` and the fungible evidence is destroyed. Two fixes:

- **Record evidence, derive the label.** Store _what was observed_ — SEP-41 signature match,
  SEP-50 11-function match, event-spec prefix topics, SCVal type tags seen — and derive the
  display category from it. A contract can then legitimately be both, and "both" is
  representable rather than a coin flip. Note the F-C anomaly this would have caught: 11
  `Fungible`-verdict contracts emit `transfer` with **`address`** data, which is neither SEP-41
  nor SEP-50 shaped.
- **Match on signatures, not names.** wallet-backend's `matchSEP41Spec` compares full
  signatures including parameter names and positions. Our own code already notes that `balance`
  returns `u32` for NFT and `i128` for fungible and that discrimination by signature was "left
  to a future refinement" — this is that refinement. Names alone are why a contract emitting a
  perfectly canonical `transfer` with a `String` first argument (`CB2SIYGH…`) sails through
  classification and dies at the parser.

### 5. Sequencing note for 0392

F-B changes the cost/benefit the task is currently reasoning about. The parser gate (gap 1) is
not a research problem needing a design breakthrough — **28 of 66 contracts are decodable from
data already on chain**, and the remaining 38 are explainable rather than mysterious. That is a
much cheaper first move than it looked, and it is also what supplies gap 3 with a real contract
to observe. It reinforces the existing "0512 first" ordering, but suggests the first slice of
0512 should be _keep and use the spec entries we already fetch_, before any bespoke-ABI work.

---

## Corrections to previously established findings

| Previously held                                                                                              | Status                            | Evidence                                                                                                                                                        |
| ------------------------------------------------------------------------------------------------------------ | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "Fungible and NFT `transfer` are byte-identical (`i128` both ways); only WASM classification separates them" | **REFUTED**                       | F-C — zero `i128` among NFT-verdict transfers; zero `u32`/`map`/`vec` among fungible-verdict. Clean separation on production data.                              |
| "Real NFTs use i128" (sibling note, refuting the i128 discriminator 0-3)                                     | **REFUTED**                       | Same measurement. SEP-50 says `TokenID` "should be an unsigned integer"; the reference library uses `u32`.                                                      |
| "Canonical [NFT transfer] is `(from, to, token_id)`" (0392 F4)                                               | **Imprecise**                     | SEP-50 puts `token_id` in **data**, not topics: topics are `("transfer", from, to)`, data is `TokenID`. Conflating the two is part of why shape matching fails. |
| SEP-48 v1.1.0 "Updated 2025-04-16"                                                                           | **Stale header**                  | Event support merged 2025-07-31, PR #1766. The preamble was not bumped.                                                                                         |
| SEP-48 `params<50>`                                                                                          | **Spec prose ≠ XDR**              | The `.x` file and generated Rust both have `params<>` (unbounded).                                                                                              |
| "5 of 5 modern SEP-50 contracts declare event specs; 1 of 8 non-conformant do"                               | **Consistent, and now explained** | F-A: the split is SDK version, not conformance. Below soroban-sdk 23 it is impossible; above it, ~half opt out.                                                 |
| SEP-47 might have progressed                                                                                 | **No**                            | Still Draft 0.1.0; last substantive commit 2025-04-16; no open PRs; zero observed adoption.                                                                     |

## Open questions

- Can `experimental_spec_shaking_v2` strip an event spec for an event that **is** reachable?
  If yes, spec absence stops being evidence of anything. **UNVERIFIED** — needs a build test.
- Does `#[contracttrait]`'s `spec_export = false` default mean deployed OpenZeppelin NFTs omit
  the 11 trait functions from their spec? If so, function-set matching degrades for exactly the
  family we most want to match. **UNVERIFIED** — needs one deployed OZ contract inspected.
- What are the 11 `Fungible`-verdict contracts emitting `transfer` with `address` data? Neither
  SEP-41 nor SEP-50 shaped; likely name-registry contracts mislabelled by the name classifier.
- `consecutive_mint` range events — is any mainnet contract using them, and how many tokens are
  currently invisible because of it?
- The **470 `(null)` signatures across 5 contracts** (0392 F4's largest single slice) remain
  unexplained. That is a decode failure _upstream_ of any classification question — the topic
  never resolved to a symbol at all — and none of this research touches it. It should be
  diagnosed before the cascade is designed around it, because it may be our bug rather than a
  contract's shape.
- Re-classification on Wasm upgrade remains unanswered by any published source (carried over
  from the sibling note; still open).
- Reservoir's `kind` field and OpenSea's `contract_standard` enum members are **UNVERIFIED** —
  first-party confirmation was not reached. Neither changes the direction, since Alchemy and
  graph-node already supply the pattern.

## Sources

**Specs (primary text + git history):**
[SEP-48](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0048.md) ·
[SEP-47](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0047.md) ·
[SEP-50](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0050.md) ·
[SEP-46](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0046.md) ·
[SEP-41](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md) ·
[open PR #1994](https://github.com/stellar/stellar-protocol/pull/1994) ·
[CAP-46-05](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-05.md) ·
[CAP-46-12](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-12.md) ·
[CAP-0057](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0057.md)

**XDR:**
[Stellar-contract-spec.x](https://github.com/stellar/stellar-xdr/blob/curr/Stellar-contract-spec.x) ·
[Stellar-ledger-entries.x](https://github.com/stellar/stellar-xdr/blob/curr/Stellar-ledger-entries.x) ·
vendored `stellar-xdr` 27.0.0 `src/generated/sc_spec_event_*.rs`

**SDK / CLI:**
[rs-soroban-sdk PR #1473](https://github.com/stellar/rs-soroban-sdk/pull/1473) ·
[PR #1524](https://github.com/stellar/rs-soroban-sdk/pull/1524) ·
[v23.0.0 release](https://github.com/stellar/rs-soroban-sdk/releases/tag/v23.0.0) ·
[`soroban-sdk/src/lib.rs`](https://github.com/stellar/rs-soroban-sdk/blob/main/soroban-sdk/src/lib.rs) ·
[`contractevent` docs](https://docs.rs/soroban-sdk/latest/soroban_sdk/attr.contractevent.html) ·
[`contracttrait` docs](https://docs.rs/soroban-sdk/latest/soroban_sdk/attr.contracttrait.html) ·
[stellar-cli `event.rs`](https://github.com/stellar/stellar-cli/blob/main/cmd/crates/soroban-spec-tools/src/event.rs) ·
[`contract.rs`](https://github.com/stellar/stellar-cli/blob/main/cmd/crates/soroban-spec-tools/src/contract.rs)

**Indexers:**
[wallet-backend README](https://github.com/stellar/wallet-backend/blob/main/README.md) +
[`validator.go`](https://github.com/stellar/wallet-backend/blob/main/internal/services/sep41/validator.go) ·
[go-stellar-sdk `contract_events.go`](https://github.com/stellar/go-stellar-sdk/blob/master/processors/token_transfer/contract_events.go) ·
[stellar-expert-explorer](https://github.com/stellar-expert/stellar-expert-explorer) ·
[stellar-dbt-public](https://github.com/stellar/stellar-dbt-public) ·
[js-stellar-sdk `event_spec.ts`](https://github.com/stellar/js-stellar-sdk/blob/main/src/contract/event_spec.ts)

**EVM precedent:**
[ERC-165](https://eips.ethereum.org/EIPS/eip-165) ·
[ERC-721](https://eips.ethereum.org/EIPS/eip-721) ·
[ERC-1155](https://eips.ethereum.org/EIPS/eip-1155) ·
[ERC-20](https://eips.ethereum.org/EIPS/eip-20) ·
[ERC-5269](https://eips.ethereum.org/EIPS/eip-5269) ·
[ERC-6551](https://eips.ethereum.org/EIPS/eip-6551) ·
[Alchemy NFT API FAQ](https://www.alchemy.com/docs/reference/nft-api-faq) ·
[blockscout `token_transfers.ex`](https://github.com/blockscout/blockscout/blob/master/apps/indexer/lib/indexer/transform/token_transfers.ex) +
`apps/explorer/lib/explorer/chain/token.ex` ·
[graph-node `add_deployment_errors` migration](https://github.com/graphprotocol/graph-node/blob/master/store/postgres/migrations/2020-04-10-111111_add_deployment_errors/up.sql) +
[`schema.rs`](https://github.com/graphprotocol/graph-node/blob/master/graph/src/data/subgraph/schema.rs) ·
[subgraph manifest](https://github.com/graphprotocol/graph-node/blob/master/docs/subgraph-manifest.md)

**Library:**
[OpenZeppelin/stellar-contracts](https://github.com/OpenZeppelin/stellar-contracts) `packages/tokens/src/non_fungible/` ·
[Stellar docs: OpenZeppelin](https://developers.stellar.org/docs/tools/openzeppelin-contracts)

**Measured this session:** mainnet via stellar-cli 26.0.0 against a public RPC (66 contracts,
`contract info meta` + `contract info interface`); production ClickHouse via `chq`
(`soroban_contracts`, `soroban_events`, `nfts`), deduped with
`argMax(contract_type, wasm_uploaded_at_ledger)` per the F6 trap. Raw TSVs in the session
scratchpad.
