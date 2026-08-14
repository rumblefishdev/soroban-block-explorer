---
id: '0432'
title: 'RESEARCH: build-vs-adopt for the ingest path — the question ADR 0004 never asked'
type: RESEARCH
status: backlog
related_adr: ['0004', '0029', '0044', '0047']
related_tasks: ['0430', '0431', '0415', '0427']
tags:
  [
    priority-medium,
    effort-medium,
    architecture,
    layer-indexer,
    layer-xdr-parsing,
    fresh-eyes,
  ]
links:
  - https://github.com/stellar/stellar-etl
  - https://developers.stellar.org/docs/data/analytics/hubble
history:
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      Spawned from a fresh-eyes review after 0430 (deployer wrong for two months)
      and 0431 (four library modules with zero call sites). The pattern is not
      "we wrote a bug" — it is "we keep re-implementing things that already
      exist, and no decision record shows anyone weighed the alternative".
      **ADR 0004 asked the wrong question.** It is titled "Rust-only XDR parsing"
      and its Alternatives Considered section weighs Rust vs TypeScript — i.e.
      *which language do we write our own parser in*. Whether to write one at all
      was never on the table. Grepped every ADR for `stellar-etl`, `hubble`,
      `horizon ingest`, "existing indexer": **zero hits**.
      This task is not "rip it out and adopt". The pipeline works and carries
      real domain logic. It is to write down, once, what the alternatives
      actually offer and where our own code is genuinely load-bearing — so the
      next re-implementation is a decision instead of a reflex.
---

# Build-vs-adopt for the ingest path

## Why this exists

Two findings in one day, same root:

- **0430** — `deployer_id` wrong in production for two months. Cause: a
  hand-rolled traversal that misses contracts created inside a contract call.
- **0431** — the library we already compile (`stellar-xdr` 27.0.0) ships
  `tx_auths`, `tx_hash`, `ledgerkey`, `num128`, `num256`, `scval_conversions`,
  `str`, `scval_validations`. **Zero call sites** for any of them. We hand-rolled
  equivalents, including the transaction-hash computation.

Neither is a coding mistake. Both are the same architectural reflex: _write it
ourselves_. Nothing in the ADR record shows that reflex was ever examined.

## Tooling landscape — verified 2026-07-22, including maintenance status

**Two corrections to what this project has assumed:**

1. **`stellar/go` was ARCHIVED on 2025-12-16.** Horizon and Galexie moved to
   their own repos (`stellar/stellar-horizon`, `stellar/stellar-galexie`).
2. **Horizon has an announced EOL** — feature-frozen, and its `result_meta_xdr`
   field is marked "to be deprecated in Q3". The self-decode escape hatch we
   rely on is closing.

| tool               | what it is                                                                        | maintained                             | Soroban era                                                                                                      | interface                      | cost                                                      | oracle?                                                       |
| ------------------ | --------------------------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------ | --------------------------------------------------------- | ------------------------------------------------------------- |
| **Galexie**        | exports raw `LedgerCloseMeta` XDR to object storage — **what we already consume** | active, `galexie-v27.0.0` (2026-06-10) | yes, by construction                                                                                             | CLI/daemon + public data lakes | free (AWS Public Blockchain lake is free)                 | **strongest — the only truly independent ground truth**       |
| **Hubble**         | SDF-hosted BigQuery dataset, full history, bronze/silver/gold                     | active (dbt v1.15.52, 2026-07-21)      | **yes, extensively** — 6 bronze + 7 silver Soroban tables incl. `evicted_keys`, `ttl`, `history_contract_events` | hosted SQL                     | SDF pays storage, **we pay query**                        | **best practical** — carries raw XDR _next to_ decoded values |
| **stellar-etl**    | Go CLI extracting history; **powers Hubble's extraction layer**                   | active, v2.8.23                        | yes (`export_diagnostic_events`, contract code/data)                                                             | CLI                            | free (own compute)                                        | **NOT independent of Hubble — same codebase**                 |
| **Stellar RPC**    | JSON-RPC to live state + recent history                                           | active                                 | yes, native                                                                                                      | JSON-RPC                       | **no free SDF mainnet endpoint** (testnet/futurenet only) | authoritative but **~7-day retention**                        |
| **Horizon**        | REST + classic ingestion                                                          | **EOL announced**                      | **essentially no**                                                                                               | REST                           | free                                                      | **no — cannot validate a Soroban indexer**                    |
| **stellar.expert** | explorer + analytics                                                              | active but **third-party**             | yes                                                                                                              | REST, 5 req/s                  | free                                                      | **no — see below**                                            |

### stellar.expert is not a valid oracle — this is the 0256 trap explained

- **Third-party**, org `stellar-expert`, not `stellar`. Not listed on the docs'
  Indexers page.
- **Derivation is closed-source** — the public repo only reads `contract.creator`
  out of their database; nothing that computes it is published.
- **Their own two API specs contradict each other**: one types `creator` as
  `^G[A-Z2-7]{55}$` (accounts only), the other as `oneOf[Account, Contract]`.
- **`ContractInfo.creator` has no description field at all**, while
  `AccountInfo.creator` does.

So when 0256 saw stellar.expert disagree with us, neither side had a written
definition. That is why the disagreement was dismissed instead of investigated.

### What the documentation actually says

Exactly one "source of truth" statement exists in the entire docs corpus:

> "The source of truth should always be the XDR defined in the protocol."
> — getLedgerEntries API reference

**No official document recommends validating an indexer against any hosted
service.** The docs point at raw protocol XDR — i.e. Galexie output, with
Hubble's `*_xdr` columns as the queryable proxy.

### Deprecated — avoid

`stellar/go` (archived), `stellar-deprecated/horizon`, `soroban-rpc` packages
(use `stellar-rpc`), `js-soroban-client` (use `js-stellar-sdk`), Horizon
overall, Horizon `result_meta_xdr` (Q3), Hubble `token_transfers_raw.amount`
(**numerically wrong for non-7-decimal tokens**), Hubble `history_assets.id`.

## What actually exists upstream (verified 2026-07-22)

| tool                        | what it is                                                                                                                                                                                                                               | fit for us                                                                                                                                              |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`stellar-etl`** (SDF, Go) | data pipeline reading **`LedgerCloseMetaBatch` XDR from a datastore** — the exact format Galexie produces and we consume — exporting ledgers, transactions, operations, effects, assets, trades, diagnostic events, ledger-entry changes | reads the same input as us, emits most of what we materialize. CLI/Docker, not a library                                                                |
| **Hubble** (SDF)            | public BigQuery dataset, complete historical record of the network, maintained by SDF                                                                                                                                                    | not real-time (intraday batches), query-cost on the consumer. Wrong for serving an explorer; **plausible as a correctness oracle or a backfill source** |
| **Horizon**                 | full-fat indexer + REST API                                                                                                                                                                                                              | heavy; wrong shape for a Soroban-first explorer, but its ingest logic is a reference implementation                                                     |
| **`stellar-xdr` crate**     | the decoder itself                                                                                                                                                                                                                       | **already a dependency** — see 0431                                                                                                                     |

The uncomfortable observation: `stellar-etl` consumes the same `LedgerCloseMetaBatch`
files from the same exporter and produces most of the same entities. We built a
Rust equivalent without a written comparison.

## Questions to answer (this is a research task — output is a decision note)

1. **What does our pipeline do that `stellar-etl` does not?** Candidates: Soroban
   contract classification, NFT detection, LP analytics, net-settled value,
   asset-identity surrogates. If that list is long, building was right and the
   answer is "keep, and document why" — which is itself worth having.
2. **Where is our code merely re-doing upstream work?** 0431 lists the library
   modules. The same audit is owed to the extraction layer: how much of
   `xdr-parser` is protocol decoding (upstream's job) versus product semantics
   (genuinely ours)?
3. **Hubble as a correctness oracle — ANSWERED 2026-07-22: yes, and it holds
   Soroban data we do not.** Its Bronze tier lists `Contract Code`,
   `Contract Data`, `History Contract Events`, `TTL`, **`Evicted Keys`** and
   `Restored Key`, alongside the classic history tables. This matters twice over:
   - It is a genuine third opinion. 0256 showed Horizon and stellar.expert can
     both mislead on the same field — Horizon reports the inner tx source for
     fee-bump envelopes (identical to our stored value, so the check was a
     tautology), and stellar.expert's `creator` is the factory operator.
   - **`Evicted Keys` / `TTL` are entities we do not track at all.** Soroban
     archives contract instances when their TTL expires; we have no notion of
     it. This is a candidate explanation for the 54 contracts carrying no
     deployer and no wasm_hash while emitting live token events (408 transfers)
     — worth checking against Hubble before assuming a parser defect.
     Remaining question is cost and latency, not capability.
4. **Is the Lambda-per-ledger shape a constraint we chose or inherited?**
   `stellar-etl` is batch/CLI. Our shape is serverless. Which of our operational
   pains (cold starts, 62× write amplification in `accounts`, repair mops) follow
   from that choice rather than from the domain?
5. **Which design patterns are we re-inventing?** Not framework ceremony — the
   concrete ones: is there an upstream notion of "ledger entry change → typed
   fact" that we re-model? Does `stellar-etl`'s schema answer questions our
   schema struggles with (0378 God-payload, 0414 `stage.rs` split, 0418 asset
   vocabulary all circle the same modelling problem)?

## Explicit non-goals

- **Not** a proposal to replace the pipeline. It works, it is deployed, it
  carries domain logic no generic tool has.
- **Not** a language rewrite. Go-vs-Rust is not the question; ADR 0004 already
  answered it for the wrong reason and the answer happens to be fine.
- **Not** a hexagonal/DDD ceremony exercise — 0418 already rules that out for
  this codebase.

## Acceptance Criteria

- [ ] A written decision note: for each upstream tool, what it would give us and
      what it would cost, with the verdict and the reason.
- [ ] An honest inventory of `xdr-parser`: which parts are protocol decoding
      (candidates for upstream) and which are product semantics (ours to keep).
- [ ] A verdict on Hubble as a validation oracle, with a sample query proving or
      disproving it can answer a question Horizon cannot (suggested: the
      fee-bump deployer case from 0430).
- [ ] ADR written or ADR 0004 amended — so the next person inherits a decision,
      not a reflex.
- [ ] Docs updated — `N/A` until a decision changes the architecture.
- [ ] API types regenerated — `N/A`.

## Note on method

Do this with the library and the upstream repos open, not from memory. The whole
reason this task exists is that four modules sat unused in a dependency we
already compile, and nobody looked.
