---
id: '0542'
title: 'FEATURE: one definition of a token movement — shared by every decoder and reader'
type: FEATURE
status: backlog
related_adr: []
related_tasks:
  [
    '0540',
    '0541',
    '0383',
    '0323',
    '0453',
    '0503',
    '0409',
    '0424',
    '0376',
    '0392',
    '0512',
  ]
tags:
  [
    'xdr-parsing',
    'clickhouse',
    'indexer',
    'phase-future',
    'effort-large',
    'priority-high',
  ]
links:
  - crates/xdr-parser/src/event_filters.rs
  - crates/xdr-parser/src/asset_transfers.rs
  - crates/xdr-parser/src/nft.rs
  - crates/db-clickhouse/src/persist/stage.rs
history:
  - date: 2026-09-07
    status: backlog
    who: karolkow
    note: >
      Spawned from 0540's deep review (seven lenses + judge). One root cause
      with several symptoms: `parse_token_event` is consumed by three write
      paths with three different trust policies, and every consumer assumed
      one topic shape per verb. 0540 fixed the two symptoms that reach
      `asset_transfers`; the rest is here.
  - date: 2026-09-10
    status: backlog
    who: karolkow
    note: >
      Measured how many contracts publish event declarations, since the
      canonical-source finding rested on an unquantified "not every contract
      publishes one". 761 of 763 wasm hashes resolved against mainnet: 24.8%
      of a universe sample declare events, 35.3% of token contracts, but only
      12.2% of non-SAC token-event VOLUME — and non-SAC is 0.098% of that
      volume. Adoption rises 32% to 69% across the emitter median upload
      ledger. Bounds steps 1 and 6 rather than changing them.
---

# Token-event decoder — shape inventory, trust policy, re-runnability

## Summary

Thread 99 interpretation decision (2026-09-07):
[NFT interpretation policy](../archive/0540_FEATURE_lossless-value-flow-index/notes/T-nft-interpretation-policy.md).
Distinguish declared amount, NFT identity and unresolved payload; use
event-specific, historical-version evidence rather than integer signedness
or today's name-only contract classification. Measured 2026-09-07 (policy
note, "Measured exposure"): zero bespoke `i128` token events from
NFT-classified contracts in two 500 k-ledger windows, and the exposure is
bounded to the collection's own `asset_id` — so it does **not** gate 0540's
backfill (task owner, thread 114 A). Implementation is step 6 below.

Post-merge 0540 correction (2026-09-07): unsigned scalar NFT IDs no longer
become fungible amounts. The reproducible exploit and limits are recorded in
[0540's regression note](../archive/0540_FEATURE_lossless-value-flow-index/notes/S-nft-amount-regression.md).
**Still required before claiming NFT-safe numeric aggregation:** resolve
bespoke i128 token-ID ambiguity consistently for live and historical replay.
The existing NFT classifier is not yet part of the value-flow decision, and
current database verdicts must not silently classify pre-upgrade events.

Make the token-event decoder the single place that knows (a) which topic
shapes exist on mainnet and (b) who is allowed to label an event with an
asset — and make every decoder-fed table re-runnable after a decoder fix.
Today three write paths consume `parse_token_event` with three policies, the
decoder knows one shape per verb by fixed position, and a fix to it cannot be
re-applied to history without a full S3 pass.

## Context

0540's review found that the SEP-41 / `soroban-token-sdk` shape
`[mint, admin, to]` (3 169 events, 54 emitters, in ledgers
64 000 000–64 100 000) was decoded with the admin as the recipient, and that
a token verb in an unknown shape was dropped silently. Both are fixed for the
value-flow tables on 0540's branch. The same class remains elsewhere:

| Where                                               | What                                                                                                                                                               | Origin       |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------ |
| `stage.rs::derive_token_event` (Tier-2 presence)    | no SAC emitter gate — a foreign contract's `"USDC:…"` event still lists under real USDC on the asset page                                                          | pre-existing |
| `nft.rs:372-376` (`collect_addresses`)              | takes the admin as owner for a 3-topic NFT `mint` → `nfts.current_owner_id`                                                                                        | pre-existing |
| every decoder-fed version-less `ReplacingMergeTree` | after a decoder fix, old and new rows share a key and differ in content; nothing says which is which (13 tables incl. 0540's 3)                                    | pre-existing |
| `process.rs` per-ledger reject `error!`             | no `alarm` field, so no CloudWatch metric; baseline on production is ~150 rejects per 500 000 ledgers, so `> 0` is no threshold                                    | 0540         |
| `<invalid-utf8>` placeholder                        | 8 decoders persist the literal as data (`scval.rs:51,55` → `soroban_events.signature`; `operation.rs:432,464,588`; `contract.rs:162,174,231`; `invocation.rs:508`) | pre-existing |

Consequence of 0540's fix living in the shared parser: from that deploy on,
the recipient of an admin-shape mint is also a `transaction_participants`
row; history stays as it was until a re-parse.

## Measured on production, 2026-09-08 — the `nft.rs` defect, with a witness

The row above was carried from 0540's review as a code reading. It now has a
concrete case, found because 0540's read path refused to trust it.

**Transaction `6338289864338569526`, ledger 56 162 678, operation position 196,
collection `CC23DRQPZAUP5MRMPDFGU5R4ISZRSCWCP4TIED2ZTVJLQCPCNDLSALAD`.** Two
pieces minted. The chain event (`soroban_events`, indices 6 and 7) reads:

```
[sym "mint", address GDJWENY5…ALAD, address CD4FTCAP…N3TX]
             ^ admin                ^ recipient (a CONTRACT)
```

| Table                            | Recipient recorded            | Correct? |
| -------------------------------- | ----------------------------- | -------- |
| `asset_transfers` (0540 decoder) | `CD4FTCAP…` (`to_kind = 'C'`) | yes      |
| `nft_ownership` (`nft.rs`)       | `GDJWENY5…` — the ADMIN       | **no**   |

The mechanism is one comparison. `try_parse_mint` calls
`extract_args(topics, data, n_addrs = 1)`, whose shape-A arm tests
`remaining_topics.len() >= n_addrs` and then takes `remaining_topics[..1]`. For
`[mint, to]` that is the recipient; for `[mint, admin, to]` the length test still
passes and the FIRST address wins, so the admin is stored as the owner. `>=`
where the shape needs `==` plus an address-count branch — the same rule 0540
put into `asset_transfers.rs`.

**Age:** `nft.rs` was written on 2026-04-01 (task 0026) against
`SEP-0050 pattern: topics = [Symbol("mint"), Address(to)]`, which its own doc
comment still states. Every admin-shape mint since has been attributed to the
admin — five months of `nfts.current_owner_id` and `nft_ownership.owner_id`.

**Not a rogue contract.** `[mint, admin, to]` is the `soroban-token-sdk` /
SEP-41 shape. Both shapes are standard; the decoder knew one of them.

**How it surfaced.** 0540's cell names a moved piece only when the count of ids
`nft_ownership` returns for `(collection, transaction, new owner)` equals the
number of pieces the account moved. Here `asset_transfers` says two pieces to
`CD4FTCAP…` and `nft_ownership` has none for that owner (its two rows sit under
the admin), so the counts disagree and the cell collapses to `+2 NFT` with no
ids rather than linking to pieces attributed to the wrong owner. Three
transactions on production behave this way today; the four larger multi-piece
mints (10, 8, 5, 5 pieces) agree and are named. So the read path is already a
live detector for this defect — worth keeping in mind when step 2 lands, since
those cells start naming pieces the moment `nft.rs` agrees.

## Re-scoped 2026-09-08 — one definition, not an inventory of shapes

The task began as "inventory the shapes the decoder meets and decide a trust
policy". A systematic sweep for CONTRADICTIONS — places where two parts of the
system answer the same question differently on real data — showed the shapes
are the symptom. The disease is that **nothing owns the answer**: the same
question is decided independently in four to five places, with different rules,
written months apart.

### "Is this non-fungible?" — four rules

| Where                      | Rule                                                                | Written    |
| -------------------------- | ------------------------------------------------------------------- | ---------- |
| `classification.rs:102`    | WASM exposes one of 5 function names                                | 2026-04-21 |
| `nft.rs:321`               | payload type is not `void`/`map`/`vec`/`error` — **accepts `i128`** | 2026-04-01 |
| `asset_transfers.rs:90`    | payload is an UNSIGNED scalar — **rejects `i128`**                  | 2026-09-07 |
| `balance_changes.rs` (API) | `amount IS NULL`                                                    | 2026-09-08 |

Rows two and three return OPPOSITE verdicts on the same event. Measured
exposure today: **0** movements carrying an amount from an NFT-classified
contract, across the live and one historical partition — so the contradiction
is latent, and step 6 below is where it gets settled.

`nft.rs` does not merely disagree; it has its own topic and payload parser
(`extract_args`, `topic_address_value`) and shares nothing with
`event_filters::parse_token_event`, which `asset_transfers` and the presence
tables both use.

### "Which address kind counts?" — four more

| Where                   | Rule                                                    |
| ----------------------- | ------------------------------------------------------- |
| `stage.rs:2690`         | `len <= 56 && starts_with('G')` (private helper)        |
| `stage.rs:819`          | the same rule **re-spelled inline**, next door          |
| `value_flow.rs:200`     | first character of the StrKey — **every kind accepted** |
| `api/common/path.rs:79` | exactly 56 chars + base32; `M` rejected                 |

So `asset_transfers` records `L`, `B` and `C` endpoints that the presence
tables discard — the schema documents 16–22% of endpoints as non-`G`. That is a
recorded decision, not a bug, but nobody owns it in one place, and the invocation
path (`stage.rs:1930`) already keeps `C`, which shows the omission is an accident.

### Measured, 2026-09-08 (production)

> **Correction, same day.** An earlier version of this table read
> `contract_type = 1` as `Fungible`. The enum is `Token = 0, Other = 1,
Nft = 2, Fungible = 3` (`domain/src/enums/contract_type.rs:23`), so `1` is
> **`Other`** — "the classifier recognised nothing", not "the classifier
> disagreed". The corrected reading is weaker as a contradiction and stronger
> as evidence for [[0512]]: the two sides do not contradict each other, one of
> them simply never had an opinion. Re-measured with the right values:
> non-fungible movements split **410 in 12 collections the classifier calls
> `Nft`** (agreement) against **136 in 3 it calls `Other`**, and asset-row
> coverage for contracts it calls `Fungible` is **4 423 of 4 423 — complete**.

| #   | Contradiction                                                                           | Exposure                                                                                                                                                          |
| --- | --------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | decoder says non-fungible, classifier says `Other` — it recognised nothing              | **136 movements, 3 collections**                                                                                                                                  |
| 2   | `nft.rs` credits the ADMIN, `asset_transfers` credits the recipient                     | witness above; 26 movements collapse in the read                                                                                                                  |
| 3   | NFT owner is a CONTRACT and the API resolves owners only via `accounts`                 | **339 of 1 089 owners (31%)** — all 339 resolve in `soroban_contracts`, so it is a read-side omission, not missing data (belongs to [[0376]], measured there too) |
| 4   | a `C` transfer endpoint with no `soroban_contracts` row                                 | 4 of 2 371                                                                                                                                                        |
| 5   | a token with >1 ownership event in one ledger — order undecidable                       | **88** ([[0424]])                                                                                                                                                 |
| 6   | `i128` — token id or amount                                                             | 0 today, latent                                                                                                                                                   |
| 7   | `M…` inside an event TOPIC is split by `value_flow` and dropped by `derive_token_event` | 0 today (no `M…` has ever appeared in a topic); the 56 muxed ids in a live partition all come from the envelope, which is the designed path                       |

Checked and found CONSISTENT, so nobody re-derives them: transfer recipients vs
`transaction_participants` (0 missing over 294 264 live and 232 523 historical
edges), `nfts.current_owner_id` vs its own ownership history (0 of 13 955),
native's surrogate (one convention, 0 empty-string rows), `canonical_id` vs
`asset_route_token`, and the three `decimals` paths.

Mitigated outside this task, in 0540, because it was already on a live
surface — but the mitigation is a PLASTER and is named as one here so it gets
removed rather than inherited. The cell stopped linking an asset whose
`assets` row is missing (4 of 51 421 fungible assets in a historical
partition). It did not ask WHY the row is missing.

**Why it is missing, traced 2026-09-08.** An `assets` row for a bespoke token
is created by `stage.rs:2119-2146` for contracts whose verdict is `Fungible`,
and that rule is complete: 4 423 of 4 423. The four dead links are contracts
the classifier calls **`Other`** — yet each one emits `{amount}` transfers, so
the chain has already demonstrated they are fungible tokens. The registry keys
on a **guess at the WASM's function names**; the evidence of what the contract
actually did is never consulted.

**The fundamental fix**, and it belongs to this task's "one definition":
register an asset from the EVIDENCE — a contract that moved a fungible amount
is a fungible asset — not from a name match. That rule is self-healing for
history, because `asset_transfers` carries the evidence for the whole range
once the backfill lands, and it removes 0540's plaster along with the
`resolves_on_asset_page` flag that exists only to route around the gap.
Sequenced after [[0512]], which is the same question asked of the classifier
itself.

**Re-measured, 2026-09-09 — the "four" were two defects wearing one symptom,
and the first count of them was wrong.** A full-table anti-join, no partition
list and no sampling: **73 contracts move value with no `assets` row**. They do
not mix — 48 emit only movements carrying an amount (118 movements), 25 only
non-fungible ones (908), none both. Only the 48 reach this section; a
non-fungible movement links to the NFT pages, which answer for collections
`assets` has never heard of.

The 48 split by what the classifier said, and the two halves are different
defects:

| Cause                                                                       | Movements | Contracts | Classifier said |
| --------------------------------------------------------------------------- | --------- | --------- | --------------- |
| Fungible token never registered — the registry-by-name gap above            | 91        | 46        | `Other`         |
| `i128` token id stored as an `amount` — [[0540]]'s correction, step 6 below | 27        | 2         | `Nft`           |

**Every count here is a mid-backfill snapshot and rises as partitions land.**
The same query read 28 orphans / 563 rows earlier the same day (recorded from
the 0374 side in [[0512]]) and 73 / 1 026 hours later. Cite the method, never
the number.

**A first pass at this reported 8 contracts and 45 movements. That was wrong**,
and the way it was wrong is worth keeping: it enumerated partitions from a
`system.parts` snapshot and then anti-joined them one by one, so the three
partitions the backfill filled while the measurement ran were never visited.
The undercount was entirely in the `Other` half (6 of 46 contracts, 18 of 91
movements); the `Nft` half was exact, because those two collections are
confined to partitions the snapshot happened to include. Against a table being
written, a partition list captured up front is stale before the query ends —
anti-join the table, not a remembered list of its parts.

Sampled from the `Other` half, not claimed of all 46: two are unarguably
fungible (a single amount of 10 000 000 000 000, and a 1.3–3.8 bn spread), and
four emit only the values `0` and `1`, two movements each — which the evidence
rule cannot classify on its own, and which need step 6's event-spec evidence
rather than a registry rule.

The live window since L₀ is **0 of 8 671** distinct fungible assets, so nothing
is on screen today. The two defects must be fixed in this order: resolving the
`i128` ids first stops step 6's collections from ever reaching the registry as
fungible candidates.

**The witness this task was named for, found 2026-09-09.** The 27 movements are
not a decoder failing in isolation — they are two decoders reading the SAME
BYTES and disagreeing. The event, out of `soroban_events`, decoded:

```
topics: [{"type":"sym","value":"mint"},{"type":"address","value":"GD75WZVG…"}]
data:   {"type":"i128","value":"20"}
```

A CAP-67-shaped `[mint, to]` with a bare `i128` payload. `nft.rs` read that
`i128` as a token id and wrote `nft_ownership.token_id`; `asset_transfers` read
the same scalar as a quantity. Joined on `(contract, ledger)`, every pair agrees
on the number and disagrees on what it means:

| ledger               | `nft_ownership.token_id` | `asset_transfers.amount` |
| -------------------- | ------------------------ | ------------------------ |
| 51 827 994           | 1                        | 1                        |
| 51 827 996           | 2                        | 2                        |
| 51 859 831           | 4                        | 4                        |
| … 27 rows, all equal |                          |                          |

One definition would have made this unrepresentable. Two definitions made it
invisible: each table is internally consistent, and only the join exposes it.

**It also puts a scope limit on [[0512]]'s tier-4 discriminator.** That tier
rests on "no overlap on the scalar types", measured as `Nft`-verdict `transfer`
emitting `u32` 8 487 / `map` 71 / `vec` 3 against `Fungible`'s `i128` 797 376.
These 27 events are **`mint`, not `transfer`** — the only signature these two
collections ever emit — so they sit outside what that measurement covered, and
they are `Nft`-verdict carrying a bare `i128`. The overlap the tier rules out
does exist; it lives on the verb the sample did not include. Re-measure per
verb before the cascade relies on it.

Bounded, checked the same day: **0 of 136 `Nft`-verdict contracts have an
`assets` row**, so the anti-join above already sees every collection in this
state — there is no larger hidden population.

### The canonical source is on-chain, and the parser throws it away (2026-09-09)

Everything above works around one absence: nothing tells us what a contract's
event _means_, so each consumer guesses from the label. That absence is not
real. **A contract declares its own events inside its WASM**, and the XDR
library this repo already depends on has carried the type since before this
task was filed:

```
ScSpecEntry::EventV0(ScSpecEventV0 {
    name,            // the event's name
    prefix_topics,   // which symbols lead the topics
    params,          // each: name, type, and LOCATION — topic or data
    data_format,     // how the payload is shaped
})
```

`ScSpecEventParamV0.location` is the field every question in this task keeps
running into: **is the recipient in the topics or in the data, and which
position**. It is declared by the contract, stored in code addressed by its own
hash, so it is immutable for that version and matches exactly the build that
emitted the event. It cannot be forged by a third party, and — unlike a topic
symbol — it is not a claim, it is the contract's own published interface.

**`contract.rs` reads the spec section and keeps `FunctionV0` only** (`if let
ScSpecEntry::FunctionV0(func) = entry`). Every event declaration on the chain
passes through this repo and is dropped on that line. `wasm_interface_metadata`
is already populated per wasm hash, so the ingestion half exists.

This reframes the task's steps rather than adding one. The shape inventory
(step 1) stops being a census of what we have seen and becomes a comparison
between what contracts DECLARE and what they EMIT — a disagreement is then a
finding, not a shrug. Step 6's `i128` ambiguity resolves from the declaration
instead of from heuristics. And the missing-sender family measured above can be
read the moment its declaration says where the sender sits.

Not every contract publishes one, so the label rule stays as the fallback for
those that do not — but it stops being the primary source for those that do.

### How many contracts actually publish one — measured 2026-09-10

The paragraph above ends on "not every contract publishes one" without saying
how many do. Measured against mainnet, since the declaration is not in our
schema: 763 wasm hashes resolved through the official CLI
(`stellar contract info interface --output json`, CLI 26.0.0), counting
`event_v0` entries in the returned spec. 761 of 763 resolved; the two failures
carry no spec section at all.

| population                                      | hashes | declare events | weighted by contracts |
| ----------------------------------------------- | ------ | -------------- | --------------------- |
| universe sample (300 of 4 976 stored hashes)    | 298    | **24.8%**      | **12.4%**             |
| token contracts — every `Nft` + `Fungible` hash | 516    | **35.3%**      | **19.4%**             |
| — `Fungible`                                    | 420    | 34.8%          | 19.0%                 |
| — `Nft`                                         | 96     | 37.5%          | 31.6%                 |

**The number that decides the step is smaller.** Over the contracts that
actually emit a token event — one partition (`intDiv(ledger_sequence,500000) =
128`), non-SAC emitters only, which is exactly the population this task guesses
about: **69 hashes, 161 897 events**. Half the CODE declares (35 hashes,
50.7%), but only **12.2% of the EVENTS** do: the three largest emitters
(91 243 + 24 440 + 18 213 events, 82% of the volume) declare nothing.

Two findings make the declaration worth reading anyway:

- **Every one of the 35 that declares, declares a token verb.** No case of "has
  event declarations, but not the ones needed" — the set of declared names
  always intersects `transfer` / `mint` / `burn` / `clawback`.
- **The declaration carries the missing field verbatim.** A live example:
  `name: "Burn"`, `prefix_topics: ["burn"]`, params `from address
location: topic_list` and `amount i128 location: data`, `data_format: map`.
  That `location` is the field every open question in this task runs into.

**Adoption is rising, not flat.** Split the same 69 emitter hashes at their
median upload ledger: the older half declares in **11 of 34 (32%)**
(ledgers 50 688 706 .. 63 035 973), the newer half in **24 of 35 (69%)**
(63 099 147 .. 64 347 339).

**Scale check that bounds the whole step.** That partition holds 165 924 064
token events; the non-SAC emitters above are 161 897 of them — **0.098%**.
Everything else is a SAC, whose shape is protocol, not a guess. So the
declaration would resolve ~12% of ~0.1% of today's traffic. The step is cheap
(`contract.rs` already reads the section and drops the entry on one line, and
`wasm_interface_metadata` is already populated per hash) — it is not urgent.

**Method caveats, so the numbers can be re-derived rather than trusted:**

- The CLI resolves by CONTRACT address, so it returns the contract's CURRENT
  wasm, while the hash keying the row may be older ([[0320]]). Checked against
  the stored `metadata.functions` for the same hash: 40 of 41 comparable
  emitter hashes have an identical function set; one had been upgraded
  (6 events). Drift is negligible in this population, not zero.
- The universe sample is 300 hashes ordered by `cityHash64(wasm_hash)` —
  deterministic and reproducible, not a statistical random sample.
- 201 of the first-pass fetches returned HTTP 429 and were re-fetched serially
  across three endpoints. Final coverage 761/763.
- The stored-metadata cross-check covered 41 of 69 hashes: rows whose JSON
  carries a newline break a TSV round trip. The uncovered 28 were not checked.

**What this does to the plan.** Step 1's declared-versus-emitted comparison is
worth building, but it cannot be the gate on its own at today's adoption — the
label rule stays primary for the volume and the declaration becomes the
authority wherever it exists. Step 6's `i128` resolution inherits the same
bound: it resolves from the declaration where there is one, and needs the
legacy-decoder evidence path for the rest.

### The trace view colours by label, with no emitter check (2026-09-09)

Same disease on a second surface. `ExecutionTrace.tsx` paints a row from
`eventCategory(traceEventLabel(event))`, and `traceEventLabel` is the first
topic symbol with a fall-back to the event type. Every call site passes that
string and nothing else: **any contract emitting a `transfer` symbol gets the
"token movement" colour**, whether or not an asset moved. The emitter gate that
`asset_transfers` applies has no counterpart here.

That is defensible for a view whose job is to render faithfully what was
announced — but the legend says "token movement", which is a claim about money,
not about a word. The declaration source above is what would let it colour by
evidence.

A related hole, measured rather than assumed: the tree's structure is rebuilt
from `fn_call` / `fn_return` labels, and the reader does not separate a host
diagnostic from a contract event, so a contract emitting a `fn_call` symbol
could inject a frame. **Zero occurrences in 309 355 024 contract events in one
partition** — never attempted, worth knowing before someone relies on the tree
as proof of anything.

### The reject inventory of the full-range backfill (2026-09-13)

The 0540 backfill decoded every token event from the ingest floor to L₀ with
today's decoder, and its completion gate proved that every event is either an
edge or a counted reject (29 of 29 partitions close to the unit — see 0540,
"Completion gate 7 passed on the full range"). So the rejects are no longer an
estimate; they are the complete list of what the decoder refuses on the whole
range. It is this task's step-1 input.

| Cause                        | Events    | What it means                                                             |
| ---------------------------- | --------- | ------------------------------------------------------------------------- |
| `unrecognised_topics`        | 6 010     | a token verb in a topic shape the decoder does not know                   |
| `unrecognised_payload`       | 1 617     | a known topic shape with a data payload it does not know                  |
| `emitter_not_sac`            | 139       | a labelled event whose emitter is not the asset's SAC — the spoofing gate |
| `no_emitter`, `no_operation` | 0         |                                                                           |
| **total**                    | **7 766** | in 6 964 ledgers                                                          |

Counted from the worker logs (`token events rejected by the asset_transfers
decoder`, one line per ledger), deduplicated per ledger because the workers'
ranges overlapped: 7 250 log lines for 6 964 distinct ledgers. Cross-checked against the database on two slices where both
sides were known: 5 = 5 in five Protocol 21 ledgers, 885 = 885 on
64 000 000–64 128 000.

Shapes already located:

- The five Protocol 21 rejects (ledgers 52 510 752, 52 510 759, 52 558 370,
  52 570 526, 52 570 585) are all `transfer`, all `unrecognised_topics`, from two
  emitters that exist in neither `soroban_contracts` nor `assets`.
- **175 events spell the verb in upper or mixed case** — `TRANSFER` 160 (three
  partitions, one emitter per partition), `MINT` 6, `Mint` 6, `Clawback` 3.
  `token_verb` matches case-insensitively, so they enter the decoder. The gate
  arithmetic does not separate how many of them decoded from how many were
  rejected. A SAC always emits lowercase and symbols are case-sensitive on
  chain, so matching them at all is a policy choice this task owns.

**The question the list has to answer is whether any of it is value we drop.**
Rejects are 0.00014% of 5.475 bn events, but volume is not the measure — one
rejected event can be a real movement missing from an account page. The method
exists and needs no new tooling: for every rejected event's transaction, run the
ledger-state witness (`operation_balance_deltas`, as `value_flow_oracle.rs`
does) on the archive file. A holder whose balance changed in that asset with no
edge is a decoder gap to fix; no balance change is a correct reject. Group by
emitter first — the per-partition counts suggest a few emitters carry most of it.

**The input is preserved on the box** as `~/bf-540/rejects-2026-09-13.log`
(7 250 lines, extracted from `~/bf-540/w{0,1,2,3}.log` with the colour codes
stripped). It is the only per-ledger record of the rejects. The rejected events
themselves are in `soroban_events`, and the ledger list is what makes finding
them there a cheap query (next section).

### What the rejects are — every event located, the value-bearing ones witnessed (2026-09-13)

**All 7 766 found in the database.** Given the ledger list from the log, a
rejected event is a token-verb row in `soroban_events` with no `asset_transfers`
row at the same `(ledger_sequence, application_order, event_index)`. Run per
partition with both sides restricted to the listed ledgers, the anti-join
returned exactly the log's count in every one of the 23 partitions that carry
rejects. The list is what keeps that join small.

**Grouped by (emitter, shape):** 307 groups from 209 emitters. Each group was
then put through the ledger-state witness (`operation_balance_deltas`, the same
comparison as `value_flow_oracle.rs`) on its archive files: one transaction per
group, three for groups of 50 or more (341 transactions, 338 ledgers), and then
**every** transaction of the groups where a balance could be involved (971
transactions, 914 ledgers). The witness also listed which storage keys of the
rejected emitter the transaction changed, which is what separates a token from
a contract that only announces one — and, for the 692 events whose balance
entries it could not value at first, dumped each entry's before and after.

| Class                                | Events    | Emitters | What the witness shows                                                                                                                                                                              | Verdict                                                                       |
| ------------------------------------ | --------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Concentrated-liquidity positions     | 3 698     | 129      | 1-topic `mint` / `burn` with `{amount, amount0, amount1, owner, …}`; the emitter's storage changes `Position` / `Tick` keys, never a balance                                                        | correct reject                                                                |
| NFT ownership                        | 2 105     | 22       | mostly `["transfer", u32 id]` → `address` (no sender); storage changes `Token(id)` / `Owner` / `Item` keys                                                                                          | **non-fungible movement dropped**                                             |
| Fungible, balance moved with no edge | **230**   | 24       | a `Balance(Address)` `i128` entry of the emitter changed with no matching edge — confirmed on **all 230**, not a sample; 28 holders (27 `G…`, 1 `C…`), ledgers 57 848 386 – 64 240 682              | **value dropped**                                                             |
| Non-standard balance layouts         | 5         | 3        | a pass token whose `Balance(Address)` is a `u64` of 1 beside an `Expiry` (3 mints); prediction-market outcome shares keyed `Balance(market, holder, outcome)` (2 burns)                             | **value dropped**; the second has no single-asset identity to record it under |
| Restated lending mints               | 218       | 6        | `["mint", address]` with `{mint_amount, mint_tokens}` beside a standard `mint` from the same contract; the `Balance(Address)` change equals that accepted edge in every one of the 218 transactions | correct reject                                                                |
| Game cards                           | 469       | 2        | `mint` / `burn` naming only the owner, data `map{}` or `void`; every one of the 469 transactions changes the owner's card-id list (`OwnerOwnedCardIds`) — the id exists only in storage             | item ownership the event cannot name                                          |
| Emitter holds no balance             | 1 041     | 24       | bridge announcements (`vec[string, i128, remote address…]`), a BTC-bridge `mint` keyed by the deposit's hash, 138 of the 139 `emitter_not_sac` labels, the 160 upper-case `TRANSFER`s               | correct reject                                                                |
| **total**                            | **7 766** | 209      |                                                                                                                                                                                                     |                                                                               |

The 230 by shape: `["mint"/"burn", address]` with `vec[i128, i128]` data (215;
the decoder takes a scalar or a map, never a vector), a 1-topic
`["mint"/"burn"]` with a bare `i128` (11; the holder is in no topic and no
payload, only in storage), a 1-topic `["mint"]` with `vec[address, i128]` (2),
a 1-topic `["transfer"]` with `vec[from, to, amount]` (1), and one
`emitter_not_sac`: a bespoke token labelling its own transfer
`"USDC:GA5ZSE…"`. The gate is right that it is not USDC; dropping it also drops
the token's real movement.

**Answers to the questions this section raised above:**

- **Is any of it value the index drops?** Yes: 230 fungible movements witnessed
  one by one, 5 more in non-standard balance layouts, and 2 105 NFT ownership
  changes. 469 game-card changes are ownership too, but the event names no id,
  so no event decoder can record them. The remaining 4 957 are correctly
  refused. Nothing is left unclassified.
- **The 175 mixed-case verbs:** 169 were rejected (`TRANSFER` 160, all one
  bridge contract; `MINT` 6; `Mint` 3). The other 6 (`Mint` 3, `Clawback` 3)
  decoded into edges. The policy question stays with step 2, but it affects
  nine events at most.

**Found on the side — a token that moves without an event.** In all three sampled
transactions of the upper-case `TRANSFER` group, a bespoke token
(`CB32ILGARL45X7IW6ROE24VPHSVRHDDQQ7GC2L67LYGB4AGZ2LU3565Z`) changed a contract's
`Balance(Address)` entry with no token event at all, and the bridge's
`TRANSFER` carries that same amount each time. Not a reject, so not in this list — a
movement no event-based index can see, and only the ledger reader catches it.

**Reproducibility.** The analysis ran as scratch scripts, not committed: the
anti-join above, a grouping by `(emitter, verb + topic types, data type or map
keys)`, and a witness binary built against `xdr-parser` at `71f1536a`
that prints, per transaction, the edges, the witness differences and the
emitter's changed storage keys, with their before and after values on request. The ledger list is re-derivable from the log.

### What this task now owns

1. **One definition of a token movement**, in `domain`, used by `nft.rs`,
   `asset_transfers.rs` and `derive_token_event`. `nft.rs` stops having its own
   parser. This subsumes step 2 below rather than sitting beside it.
2. **One rule for which address kinds count**, in one place, with the current
   per-table policy expressed against it rather than re-spelled four times.
3. **The `nft.rs` admin-shape fix** (task owner, 2026-09-08: it belongs here,
   not as a separate change) — including the re-derivation of the historical
   `nfts` / `nft_ownership` rows it has been mis-attributing since 2026-04-01.
4. **[[0409]] is absorbed**: "arm-A NFT pollution" is this same disease in
   another table — a token event written with an NFT's id parsed as an amount,
   because the writer did not consult the same definition.

Related, NOT absorbed: [[0512]] (the classifier itself), [[0392]] (the
completeness umbrella above it), [[0424]] (ownership order), [[0376]] (contract
owners), [[0486]] (the collection view). Each keeps its own outcome; this task
supplies the shared vocabulary they all read.

## Implementation Plan

### Step 1: Shape inventory as a gate

`GROUP BY (signature, JSONLength(topics), type of last topic, type of data,
keys of a map data)` over `soroban_events`, per epoch. Every combination gets
an explicit decoder status — accepted / rejected-and-counted / impossible —
checked in as a test fixture. Re-run after each protocol upgrade. The 0540
oracle (33 ledgers) has no statistical power for shapes at 1e-4 frequency;
this has.

### Step 2: One trust policy

Move the SAC emitter gate (`emitter == derive_sac(asset)`,
`sac_override_from_event_topics`) into `derive_token_event`, so the Tier-2
presence tables get it too. Decide explicitly whether
`operation_asset_appearances` / `transaction_participants` history is
re-emitted (two ~10 bn-row tables) or the exposure is documented as a
boundary. Fix `nft.rs` with the same shape rule.

### Step 3: `parser_version` on decoder-fed tables

A `parser_version` column on every decoder-fed version-less
`ReplacingMergeTree`, so a decoder fix can be re-run on a range and the 0503
tie query has something to resolve on. Repo-wide; needs a deploy window
(driver validates the row struct against `DESCRIBE`).

### Step 4: Reject alarm with a threshold

`alarm` field on the per-ledger `error!` + CloudWatch metric filter +
`FILTER_MINTED_METRICS` entry, threshold from the measured baseline.

### Step 6: `i128` token ids — semantic resolution for live and replay

Resolve the one payload the parser cannot: a bespoke NFT whose token id is
an `i128`. Evidence in this order: SEP-48 event specs from the executing
WASM version (`ScSpecEntry` event entries — `contract.rs` keeps only
`FunctionV0` today), then an evidence-backed legacy decoder for named
implementations. Live and replay must apply the same immutable evidence to
the same event or both return unresolved; never today's verdict applied
backwards. Output: `amount = NULL` for a resolved NFT id, a counted reject
for unresolved; no new column on `asset_transfers`. Tests per the policy
note's list (same `i128` under FT / NFT / missing context, packed and batch
shapes, self-transfer, executable upgrade mid-history).

### Step 5: `<invalid-utf8>` → bytes

Same fix 0540 makes for `transaction_memos.memo`: keep the bytes (hex or a
typed column), never a literal indistinguishable from real content.

## Acceptance Criteria

- [ ] A checked-in shape inventory covers every (verb, topic shape, data
      shape) seen on production; each has an explicit decoder status
- [ ] One emitter gate, applied to every consumer of `parse_token_event`;
      decision recorded on Tier-2 history
- [ ] `nfts.current_owner_id` correct for 3-topic NFT mints, verified on the
      measured instances
- [ ] `parser_version` on the decoder-fed tables; a re-run on a range is
      provably resolvable
- [ ] Reject alarm fires above the measured baseline, not at `> 0`
- [ ] No `<invalid-utf8>` literal persisted anywhere
- [ ] `i128` token ids resolved by event-spec or legacy-decoder evidence,
      identically for live and replay; unresolved is a counted reject
- [ ] **Docs updated** — `docs/architecture/xdr-parsing/**`,
      `database-schema/**` per ADR 0032
- [ ] **API types regenerated** — N/A unless the API surface changes

## Notes

Sizes from 0540's pattern-generalisation pass (estimates): gate + nft.rs ~1
day; `parser_version` ~0.5 day + window; alarm ~0.5 day; utf-8 ~0.5 day;
inventory ~1 day.
