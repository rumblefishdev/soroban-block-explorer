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
3. **Could Hubble serve as a correctness oracle?** We currently validate against
   Horizon and stellar.expert, and 0256 showed both can mislead — Horizon reports
   the inner tx source for fee-bump envelopes, stellar.expert's `creator` is the
   factory operator. A dataset built by the protocol's own maintainers is a third
   opinion with different failure modes.
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
