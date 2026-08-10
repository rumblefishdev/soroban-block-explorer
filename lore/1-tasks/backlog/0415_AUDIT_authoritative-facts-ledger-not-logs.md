---
id: '0415'
title: 'AUDIT: every authoritative fact must come from the ledger (state), not events (logs)'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0393', '0410']
tags:
  [
    'xdr-parser',
    'indexer',
    'security',
    'audit',
    'phase-future',
    'effort-large',
    'priority-high',
  ]
links: []
history:
  - date: 2026-07-20
    status: backlog
    who: karolkow
    note: 'Spawned from the net-settled redesign. Value was being derived from spoofable contract events (logs); fixed to read the ledger. The same class of bug may exist elsewhere — audit the whole indexer.'
  - date: 2026-07-20
    status: backlog
    who: karolkow
    note: 'Priority-high, NFT ownership set as the first target — largest still-live event-trust bug (nft.rs detect_nft_events → nft_ownership owner from event topics). Re-verified in official docs that Soroban events are schema-less, unenforced, non-consensus notifications (only the emitter contract_id is host-stamped).'
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      THESIS CORRECTED after a source-grounded re-audit (official CAP/SEP + mainnet
      reads only; our own repo prose explicitly barred as evidence). Two errors in
      the previous framing: (1) "events are not part of consensus" is FALSE for
      Soroban contract events — they are hashed via InvokeHostFunctionSuccessPreImage
      (CAP-0046-08 Final); (2) "ledger beats events" is FALSE for contract-issued
      assets — ContractData key/val are contract-authored exactly as event payloads
      are, so the swap changes only which hash covers the bytes. Correct axis is
      HOST-written vs CONTRACT-written. NFT ownership REMOVED as the priority-one
      migration target: no storage standard (SEP-50 Draft), keyspace not enumerable
      (CAP-0046-05 Final), entries archive (CAP-0046-12 Final) — events are the only
      possible source there. Added source matrix + full research record.
      RESEARCH ONLY — no tasks spawned for the host-vs-contract question and nothing
      scheduled for implementation; two draft task files (classifier, state
      archival) were withdrawn to .trash and their substance folded in here as
      "candidate work, not spawned". Separately re-opened 0410 (presence-path emitter
      gate) on explicit request — that one is a concrete live bug, not part of this
      open question.
---

# AUDIT: source every authoritative fact from the party the protocol forces to be honest — host vs contract

> **This task is the AUDIT itself — it does not fix anything directly, and as of
> 2026-07-21 it is deliberately RESEARCH ONLY: nothing here is scheduled and no fix
> tasks are spawned from it.** Its job: place every indexed fact on the **authorship**
> axis (host-written vs contract-written), label it, and record the evidence. The
> "Scope" list below is the starting map; the Source matrix and Research record
> further down are the current output.
>
> The task's original title and framing — _"every authoritative fact must come from
> the ledger, not events"_ — were **refuted** by a source-grounded re-audit and are
> kept only where struck through, for the trail. NFT ownership is **no longer** the
> priority-one migration target; see the matrix for why it cannot move to the ledger
> at all.

## Summary

> **THESIS CORRECTED 2026-07-21.** The original framing of this task — _"authoritative
> facts must come from the ledger, not events"_ — was built on a premise that a
> source-grounded re-audit **refuted**. The corrected thesis is below. The task
> itself is still valid and still worth doing; only its organising rule changed.

The net-settled value used to be derived from **contract events**, which a contract
authors freely — so it was spoofable, and moving it to ledger balance changes
(task 0393) was correct. But the reason it was correct is **not** "ledger beats
events". It is that for native / classic / SAC assets the **host**, not the
contract, writes those balances.

Trust decomposes into **two independent axes** that the original framing conflated:

| Axis           | Question                                            | Changes when you…                   |
| -------------- | --------------------------------------------------- | ----------------------------------- |
| **Authorship** | who computed the bytes — the host, or the contract? | change **which fact** you use       |
| **Commitment** | which hash covers them?                             | change **which container** you read |

Swapping events for ledger state moves you along the **commitment** axis only. For a
contract-issued token or an NFT, the `ContractData` entry and the `transfer` event
are authored by **the same untrusted party**. So that swap buys **zero** semantic
trust for exactly the assets it claims to protect, while costing enumerability,
archival survival, and a standard to parse by.

## The rule

> **Source every published fact from the party the protocol forces to be honest
> about it — the host for classic, native and SAC facts; the contract, labelled as
> its own claim, for everything a contract authored — because moving a
> contract-authored fact from its event to its ledger entry changes only which hash
> covers it, never who wrote it, while surrendering enumerability, archival
> survival, and a standard to parse it by.**

This is the sentence the ADR should carry. The audit's job is to place every
indexed fact on the authorship axis and label it accordingly — **not** to migrate
everything to ledger reads.

## Prior art — how other explorers split it (2026-07-20 research)

The industry confirms a **two-layer split**, which is exactly the target end-state:

- **Ledger-derived "what changed" (authoritative).** Stellar **Horizon `effects`** are
  "the specific ways the ledger was changed by an operation" — account_credited /
  \_debited, trustline_updated, trade, etc. — **computed from the ledger meta**, not
  from any log. Etherscan's **State / "Tokens Transferred"** section is the same idea
  (storage/balance diffs). This is the **fact** layer → ledger. Our net-settled value
  is the first piece of it; a fuller ledger-derived "effects"-style view is the
  natural home for the facts 0415 wants off the event path.
- **Contract's semantic narration (a log, shown AS a log).** Etherscan's **Logs tab**
  = the raw events the contract emitted, ABI-decoded — "what the contract explicitly
  communicated happened." Every major explorer keeps this, BUT it is a contract
  self-report: value/balance/ownership facts are NOT read from it.

Takeaway for us: **do NOT try to replace the events display with ledger data** — the
raw ledger (`ContractData` byte changes) is opaque without the contract ABI, so a
contract's semantic events (swap / borrow / liquidate / mint) have no ledger
equivalent for _display_. The fix is the split: **facts from the ledger (effects
style), events shown as labelled logs.** Whether to STORE the event XDR or re-decode
it is a separate storage/cost axis (task 0416), not a correctness one.

## Context

- **Verified externally** (developers.stellar.org, CAP-46-6, real mainnet txs):
  SAC/classic value moves as `AccountEntry` / `TrustLineEntry` / `ContractData`
  `Balance` changes; a contract cannot forge those. Events are notifications.
- **Docs confirm events are unauthenticated logs** (developers.stellar.org
  "Events", 2026-07-20 re-check, two independent sources):
  - Topics are **schema-less** — "there are no requirements on format of values set
    in topics". The runtime **does not inspect or enforce** any aspect of event data.
  - ~~Events are **not part of consensus**~~ — **THIS WAS WRONG (corrected
    2026-07-21).** That quote was lifted from the _diagnostic-events_ paragraph and
    wrongly generalised. Hashing has **three** different answers:
    - Soroban `CONTRACT` / `SYSTEM` events — **hashed**, via
      `sha256(InvokeHostFunctionSuccessPreImage{returnValue, events<>})` → operation
      result → `txSetResultHash` in the ledger header (CAP-0046-08, **Final**, p20:
      _"This makes the events part of the protocol"_). **Success arm only.**
    - `DIAGNOSTIC` events — not hashed, and gated on a node config flag.
    - CAP-67 **classic / fee** events — **deliberately not hashed** (CAP-0067,
      **Final**, p23) — though these are host-authored, so spoofing does not apply.
      The commitment covers _what the contract said_, never whether it is true.
      Retrievability is a separate matter: RPC keeps ~7 days (measured), so long-range
      event history must be self-ingested — which we do.
  - **The one authenticated bit:** the emitting `contract_id` is host-stamped (a
    contract can't emit _as_ another contract). But the topic/data _content_ (asset
    string, amount, `to`/`from`) is fully attacker-chosen. → the safe pattern is to
    trust the emitter id and **cryptographically bind** it (as the SAC guard does:
    `derive_sac(asset, net_id) == emitter`), never to trust bare topic content.
- **Already ledger-based (good):** balances (0331), contract metadata (on-ledger
  instance storage), and now net-settled value (this redesign).

## Scope — audit each derived fact for its source

For every fact the explorer presents as authoritative, determine LEDGER vs LOG and
flag the log-derived ones.

### PRIORITY TARGET (do first) — NFT ownership / existence / mint / transfer

The largest **still-live** instance of the same class of bug as the net-settled
value (which is now fixed). NFT existence, ownership, and mint/transfer/burn are
derived from **contract events**, spoofable end-to-end:

- `xdr-parser` `nft.rs` `detect_nft_events` → `NftEvent` with `to`/`owner` taken
  straight from event **topics** (e.g. `nft.rs` `to: Some(addrs[1].clone())`).
- `db-clickhouse` `stage.rs` writes `nft_ownership` with `owner_id` = the event's
  reported owner (`owner_account`), not a ledger entry.
- **PoC:** an attacker contract emits `["mint", G<victim>, u32:42]` moving nothing;
  the indexer records "victim owns NFT #42 of attacker's collection". No cost, no
  real state change.
- **Ledger source exists:** NFT ownership lives in the contract's `ContractData`
  (owner mapping / SEP-50 `owner_of`), so it can be re-derived authoritatively —
  the same move we made for value. This is the first fix task to spawn.

### Then — the rest

- **Token-transfer participants** (`transaction_participants` via
  `derive_token_event`) — from event `from`/`to`, or from which balances changed?
- **Contract classification** (Token / NFT / Other) — from event patterns
  (mint/transfer keyword sniffing), or from the WASM interface (`ContractCode` /
  `wasm_interface_metadata`)? (should be ledger/interface)
- **Bespoke token value** (net-settled `ContractToken` branch) — confirm the
  `Balance(Address)` bare-i128 ledger read on a real bespoke token (verification
  was pending: RPC retention + query limits).
- **SAC identity / undeployed-SAC overrides** — auth-tree derived + crypto-gated
  (`sac_override_from_event_topics`); this one IS safe (cryptographic), document as
  the correct pattern.
- Any other `detect_*_events` / `*_from_events` that asserts a fact.

## Source matrix (2026-07-21 audit result)

`H` = host-written (semantics trustworthy). `C` = contract-written (attributable,
**not** trustworthy). The verdict column is what we should do, not what we do now.

| Fact                                     | Source                                            | Author                                                | Verdict                                                                             |
| ---------------------------------------- | ------------------------------------------------- | ----------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Native + classic balances                | `AccountEntry` / `TrustLineEntry` changes         | **H**                                                 | ledger — done                                                                       |
| SAC balances (contract-held classic)     | `ContractData` `Balance(Address)`                 | **H** — SAC is a host contract, CAP-0046-06 **Final** | ledger — done                                                                       |
| Net-settled value (native/classic/SAC)   | ledger balance deltas                             | **H**                                                 | ledger — done (0393)                                                                |
| Net-settled value (bespoke leg)          | `ContractData` bare `i128`                        | **C**                                                 | keep, but **stop calling it unspoofable**; must surface as `unknown`, never vanish  |
| Custom (non-SAC) token balances          | `ContractData` `Balance(Address)`                 | **C**                                                 | convention only; ~65% of contracts we call Fungible have no such row                |
| Assets a tx touched                      | op body ∪ CAP-67 per-op events                    | **H** classic / **C** bespoke                         | union; gate the classic identity (task 0410)                                        |
| Accounts that participated               | tx/op source, claim atoms ∪ event `from`/`to`     | **H** ∪ **C**                                         | restrict or label the **C** half                                                    |
| NFT existence + ownership                | contract events                                   | **C**, irreducibly                                    | **events are the ONLY source** — no ledger alternative exists; label as self-report |
| Contract classification                  | SEP-48 typed signature sets                       | **H** (what the WASM declares)                        | signature sets, never bare names — see spawn                                        |
| Contract metadata (name/symbol/decimals) | SAC instance `METADATA` / custom instance storage | **H** SAC / **C** custom                              | label the **C** half                                                                |
| Events feed                              | `TransactionMetaV4` events                        | mixed                                                 | display as logs — legitimate                                                        |

**Why NFT ownership cannot move to the ledger** (this reverses the task's original
priority-one target): ownership keys are contract-chosen; CAP-0046-05 (**Final**)
forbids keyspace iteration (_"no support for range queries… or any sort of
iteration over the keyspace"_), so entries are not enumerable; SEP-0050 is **Draft**
and specifies **zero** storage layout; OpenZeppelin's `NFTStorageKey::Owner(u32)` is
a library convention that cannot even express the `i128` token ids already seen on
mainnet; and persistent entries **archive** (CAP-0046-12 **Final**), so historical
ownership is not readable from state at all. Events, by contrast, are permanent.

**And disambiguation by event shape is impossible by spec:** SEP-50's `transfer`
topics are byte-identical to SEP-41's — only the `data` field differs (TokenID vs
i128 amount). Topic sniffing cannot work; the typed interface must decide.

## Research record — 2026-07-21 source-grounded audit

> **STATUS: RESEARCH ONLY.** Nothing here is scheduled, and no tasks were spawned
> for the host-vs-contract question. This section is the evidence base; the
> decision about what (if anything) to build comes later. Two draft task files were
> written and then withdrawn to `.trash/` on purpose — their substance is folded in
> below so nothing is lost.

### Method — and why this run supersedes the earlier ones

Two earlier passes (an adversarial red team and a defending blue team) both leaned
on **this repo's own comments and docs as evidence for protocol behaviour**. That is
circular: our prose is one team's interpretation, and it is now demonstrably wrong in
at least two places. The third pass was run under a hard evidence rule — official
CAP/SEP documents **with their Status**, developers.stellar.org, the reference
implementations (`stellar-core`, `rs-soroban-env`, `rs-soroban-sdk`), and **direct
mainnet reads**; the repo admissible only as the _subject_ under test. Anything not
establishable that way was to be marked UNVERIFIED rather than filled in.

That rule is what caught the errors. Keep it for any future round.

### Protocol ground truth (established from official sources + mainnet)

- **Emitter is host-stamped.** `contract_event` takes no id argument;
  `record_contract_event` fills `contract_id` from the executing frame. A contract
  cannot emit _as_ another contract. (`rs-soroban-env`; CAP-0046-03 **Final**.)
- **Everything else in an event is unvalidated.** CAP-0046-08 (**Final**):
  _"There are no limits on individual events, but the total size of all events
  emitted in a transaction."_ The documented "≤4 topics, no Vec/Map" restriction is
  **not enforced** — proven by a live mainnet counterexample: contract
  `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK` emits a `swap` event
  whose second topic is a `Vec`. Topic semantics are pure convention, binding only
  on the built-in SAC.
- **Hashing has three answers** (see the corrected bullet above): Soroban
  contract/system events hashed (success arm only); diagnostic not hashed; CAP-67
  classic/fee events deliberately not hashed.
- **`ContractData` is contract-authored.** The `contract` field is host-stamped
  exactly as an event's `contract_id` is, but `key` and `val` are wholly
  contract-supplied and unvalidated (`rs-soroban-env::data_helper`). **A
  contract-written ledger entry is not more trustworthy than that contract's own
  event.**
- **No keyspace iteration.** CAP-0046-05 (**Final**): _"no support for 'range
  queries', upper or lower bounds, or any sort of iteration over the keyspace."_
  `getLedgerEntries` takes exact keys, max 200. Contract state is **not enumerable**.
- **No event count limit, only size.** Live mainnet config read during the audit:
  `txMaxContractEventsSizeBytes = 16384`, `feeContractEvents1KB = 5000`. This is a
  validator-votable setting, not a protocol constant.
- **Failed calls are excluded.** Events from a reverted sub-invocation are marked
  `FromFailedCall` and filtered out of the hashed set and the meta `events` vector,
  even when the outer transaction succeeds via `try_call`.
- **Storage standards:** SEP-0041 (fungible) and SEP-0050 (NFT) are both **Draft**
  and contain **zero** storage-layout text. The only protocol-mandated layout is the
  SAC's `Balance(Address)` (CAP-0046-06 **Final**). OpenZeppelin's key enums are a
  library convention, not a standard.
- **State archives.** CAP-0046-12 (**Final**) / CAP-0062 (**Final**): persistent
  entries reach TTL 0 and become **inaccessible**; temporary entries die
  permanently. Historical _state_ has no protocol guarantee — every accessibility
  rule is written against `currentLedger`. Historical _events_ are permanent.

### Candidate work — deliberately NOT spawned

Recorded so it is not re-discovered from scratch. **Not scheduled.**

**A. Classification fires on a single bare function name.**
`classify_contract_from_wasm_spec` matches one name (`decimals | allowance |
total_supply` → `Fungible`; `owner_of | token_uri | …` → `Nft`). Verified in code,
in the enum (`ContractType::Fungible = 3`), and on prod: the **Reflector price
oracle** `CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN` (~511k
invocations, exports `price/prices/lastprice/decimals`, no `transfer`, no `balance`)
and the Aquarius pool registry `CBKCROE56TU2FTT3C5CVN676PYVLTOQUQDHHH57GLWDY5VOKSCZPGOFN`
are both stored as `contract_type = 3`. Prod has 4,211 rows at that type.
Name matching cannot be repaired by adding names — SEP-50 and SEP-41 define
`transfer` with **byte-identical topics**, differing only in the `data` field. The
fix would be typed **signature sets** from SEP-0048 (**Active**), reusing the
`ScSpecEntry` decode already present in `contract.rs`.
Adjacent (agent-reported, unverified): ~60% of the most active event emitters have
**no contract instance** — they are deterministic SAC addresses of never-deployed
SACs, which CAP-67 emits under anyway; treating emitter as contract invents phantom
contract rows.

**C. Generalise the emitter gate — but only where it actually buys something.**
Both the adversarial and the defending review converged on this: the cryptographic
binding in `sac_override_from_event_topics` (`derive_sac(asset, net_id) == emitter`)
is the correct pattern and is currently applied in **one** place. It should be the
centrepiece of the "hardened events" path. The honest scope is narrow, though —
sweep for call sites and classify each:

- **Gate HELPS** where an event _claims an identity that can be independently
  re-derived_. That is essentially one shape: a classic/native asset claimed via its
  SEP-11 string, where the canonical SAC address is `SHA256(HashIDPreimage{networkID,
fromAsset})`. Known site: the presence path (task 0410, re-opened). Sweep for
  others.
- **Gate is a TAUTOLOGY** for bespoke tokens and NFTs: there the identity _is_ the
  emitter, so "verify emitter == subject" always passes and proves nothing. Applying
  it there would create false assurance — worse than not applying it.
- **Gate is IRRELEVANT** where the emitter is honest and the _operands_ are the lie
  (`from`/`to` harvested into participants, `to` taken as an NFT owner). Nothing
  cryptographic can help; the only fixes are restricting which emitters may create
  such rows, and labelling provenance.

Deliverable of the sweep is a table of call sites × which of those three buckets,
so we stop reasoning about "add the gate" as if it were uniformly useful.

**B. We are structurally blind to state archival.**
Eviction is reported in `LedgerCloseMeta{V1,V2}.evictedKeys<>` — a **top-level
field, sibling of `txProcessing`** — so a walk that only traverses transaction-apply
processing **cannot observe it at all**. The repo has zero references to `evicted`,
and `liveUntilLedgerSeq` from `Ttl` entries is parsed then discarded. We read
`ContractData Balance(Address)` as the holder/balance source (task 0331), so an
archived holder silently reads as absent/zero and a dead temporary balance stays
positive forever. Live mainnet settings (agent-reported, re-read before use):
`max_entry_ttl` ≈ 2 years, `min_persistent_ttl` ≈ 4 months, `min_temporary_ttl`
≈ 1 day. Landmine: CAP-0076 (**Final**, p24) — 478 mainnet keys were archived
carrying stale state during p23 and the correction _"will not be reflected in
`LedgerCloseMeta`"_, so replay alone can never see the fix.
`docs/architecture/**` contains **no discussion of state archival anywhere**
(verified) despite the pipeline depending on contract state.

### Verified findings (2026-07-21)

Independently confirmed in code + prod during this audit:

1. **Classifier is refuted at the head of the traffic distribution.**
   `classify_contract_from_wasm_spec` matches a **single bare name**
   (`decimals | allowance | total_supply` → `Fungible`). Prod: the Reflector price
   oracle `CAFJZQWS…` (511k invocations) and the Aquarius pool registry
   `CBKCROE5…` are both stored as `contract_type = 3` (`Fungible`). Neither exports
   `transfer` or `balance`. → spawned as its own task.
2. **Presence path has no emitter binding** — an event-supplied SEP-11 asset string
   maps straight onto the real classic surrogate, so forged rows can be injected
   into a real asset's transaction list. → task 0410 re-opened and re-scoped.
3. **Our own docs contradict the protocol.**
   `indexing-pipeline-overview.md:185` and `technical-design-general-overview.md:727`
   both claim _"CAP-67 contract events from `SorobanTransactionMeta.events`"_. Wrong
   CAP (Soroban contract events are CAP-0046-08), and `SorobanTransactionMetaV2` has
   **no** `events` field — they moved to `OperationMetaV2`. Our own
   `xdr-parsing-overview.md:459` states this correctly, so we contradict ourselves.
   Fix under ADR 0032.
4. **State archival is absent from our architecture docs entirely** and from the
   pipeline. → spawned as its own task.
5. **`fee` events are ~59% of `soroban_events`** (measured) — recorded in 0416.
6. Ordering ambiguity in event folds → task 0424.

## Acceptance Criteria

- [ ] **NFT ownership re-derived from the ledger** (`ContractData`), not events —
      spawned as its own fix task (the priority target). Old event-derived
      `nft_ownership` writes retired or cross-checked.
- [ ] A table: every authoritative fact → its source (ledger / log / hybrid) →
      spoofable? → remediation.
- [ ] Each log-derived authoritative fact has a spawned fix task (ledger-based
      re-derivation) OR a documented justification if the ledger genuinely lacks it.
- [ ] The bespoke `ContractToken` bare-i128 shape is confirmed on real mainnet data.
- [ ] A short ADR: "authoritative facts come from ledger state; events are logs"
      (the principle, so future work doesn't re-introduce log-trust).
