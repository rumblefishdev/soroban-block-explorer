---
id: '0061'
title: 'Read contract facts by executing their standard view functions locally'
status: proposed
deciders: [karolkow]
related_tasks: ['0620', '0621', '0617']
related_adrs: []
tags: ['soroban', 'tokens', 'classification', 'indexing']
links: []
history:
  - date: 2026-10-05
    status: proposed
    who: karolkow
    note: 'ADR created from the 0617 research and the 0620 spike.'
---

# ADR 0061: Read contract facts by executing their standard view functions locally

**Related:**

- [Task 0620: Execute contract view functions locally](../1-tasks/backlog/0620_FEATURE_execute-contract-view-functions-locally/README.md)
- [Task 0621: Classify fungible tokens by exact SEP-41 signatures](../1-tasks/backlog/0621_REFACTOR_classify-tokens-by-sep41-signatures.md)

---

## Context

A Stellar standard fixes a contract's **functions** (SEP-41 `decimals`,
`name`, `symbol`, `balance`), not where it keeps its data. The pipeline reads
raw instance storage by key name and classifies contracts by function names.
Every new author layout is a silent gap: 287 of 4,207 token contracts have no
decimals although their `decimals()` answers; key names can mislead (vault
`*Decimals` keys hold the underlying asset's scale). Recorded trouble from
name- and key-based reads: 0118, 0283, 0297, 0309, 0320, 0340, 0392, 0473,
0512, 0617.

A spike (0620) ran `decimals()` locally on all 4,207 token contracts with
`soroban-env-host`, the library validators and RPC use: 0 differences against
RPC simulation, p50 0.7 ms per call.

---

## Decision

1. Facts that a standard defines as a **view function** are read by executing
   that function locally with `soroban-env-host`, pinned to the network's
   protocol — first token `decimals`, `name`, `symbol` (0620).
2. Program bytes are stored once per WASM hash (`wasm_code`), written at
   upload; 5,235 programs are 34–65 MB compressed.
3. Ledger entries the pipeline does not hold (another contract's instance,
   persistent data — 106 of 4,207 `decimals()` calls) are read as ledger
   entries, never simulated remotely.
4. Facts a standard defines as **events** (transfers, NFT ownership) keep
   coming from events. Facts with no standard (AMM reserves, registries) stay
   per-protocol and are out of scope here.

---

## Rationale

The function is what the standard guarantees and what wallets and other
contracts call, so its answer is the token's truth by definition. Local
execution gives that answer without an external dependency, at the ledger
being indexed, deterministically. Storage parsing can only follow layouts
someone has seen.

---

## Alternatives Considered

### Alternative 1: Teach the parser more storage layouts

**Decision:** REJECTED — covers 120 of 287 missing tokens, writes wrong
scales for vaults, and grows a list that is never complete.

### Alternative 2: RPC `simulateTransaction` per contract

**Decision:** REJECTED — external dependency with rate limits (~50% of
census calls answered 429); same answers as local execution.

### Alternative 3: Per-program layout descriptor learned once

**Decision:** REJECTED — needs the program bytes anyway, cannot be learned at
upload, breaks on cross-contract programs, and is a second storage
interpreter that goes stale on upgrade.

### Alternative 4: Do not store program bytes; fetch by hash when needed

**Decision:** REJECTED — the bytes are immutable and small (0.01% of the
database); fetching ties every execution to an external service.

---

## Consequences

### Positive

- One rule for every token layout; no per-author code.
- Classification can confirm "is a token" by a successful call (0621).

### Negative

- `soroban-env-host` must be bumped with each protocol, like `stellar-xdr`.
- A new table and a one-off backfill of program bytes.

---

## Delivery Checklist

- [ ] `docs/architecture/technical-design-general-overview.md` — N/A until 0620 lands
- [ ] `docs/architecture/database-schema/database-schema-overview.md` — `wasm_code` (0620)
- [ ] `docs/architecture/backend/backend-overview.md` — N/A — API unchanged
- [ ] `docs/architecture/frontend/frontend-overview.md` — N/A — no frontend change
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` — executor step (0620)
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` — N/A — no new infrastructure
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — metadata source (0620)
- [ ] This ADR is linked from each updated doc at the relevant section

---

## References

- [SEP-41: Soroban Token Interface](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)
