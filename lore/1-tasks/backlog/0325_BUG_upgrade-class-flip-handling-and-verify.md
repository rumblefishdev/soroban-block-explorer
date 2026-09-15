---
id: '0325'
title: "BUG: a code-derived row must follow the contract's CURRENT code — class flips on WASM upgrade (contract type, NFT quarantine, soroban pools)"
type: BUG
status: backlog
related_adr: []
related_tasks: ['0320', '0283', '0374', '0518']
tags:
  [
    soroban,
    classification,
    clickhouse,
    executable_update,
    liquidity-pools,
    phase-future,
    effort-small,
    priority-low,
  ]
links: []
history:
  - date: 2026-06-24
    status: backlog
    who: karolkow
    note: >
      Spawned from 0320 research. 0320 fixes stale wasm_hash (update the field);
      it deliberately does NOT handle the rare case where an upgrade CHANGES the
      contract's class. Measured: 2 of 4,691 upgrades changed class, both on one
      contract. Deferred here.
  - date: 2026-09-15
    status: backlog
    who: karolkow
    note: >
      Widened to the general rule. A registered soroban pool was found whose
      code had been replaced by non-pool code 20 months earlier while the
      registry still presented it as a pool (0374 production verification).
      Re-measured over every upgrade: 9 of 5,296 changed what the contract is,
      across 6 contracts; exactly one derived row misrepresents its contract
      today. Decided to extend this task rather than file alongside it.
---

# BUG: class flip on WASM upgrade — handle + verify it is real

## Summary

Task **0320** corrects the stale `soroban_contracts.wasm_hash` after a WASM
upgrade ("update the field"). It deliberately does **not** handle the case where
an upgrade _changes the contract's class_ (Other ↔ Fungible ↔ Nft), because the
mainnet measurement showed this is vanishingly rare and **net-zero** across all
1,362 upgraded contracts. This task picks up that deferred edge case.

## Context — exactly where it happened (measured 2026-06-24, prod CH)

Across **4,691** `executable_update` upgrades, **only 2 transitions changed
class**, both on a single contract:

- **Contract:** `CDCN2D4OF5IHPAHUIF6RPVH654KW6LKTYKYK3IQULBBWURD7L4CDNSRO` (37 upgrades)
- **Ledger 59265674 — Other → Fungible.** New wasm `13e408b8…` exposes
  `total_supply, decimals, allowance, balance, approve, transfer, name, symbol,
mint_reward, max_supply, …` (an OpenZeppelin fungible surface).
- **Ledger 59337663 — Fungible → Other.** New wasm `f35c6fb9…` (OLD `2bd0eeb7…`)
  dropped the fungible discriminators.
- **Net deploy → current: Other → Other** (no change). 0 NFT flips ever, 0 net
  flips for any of the 1,362 contracts.

**Open question — real or a parse artifact?** Initial evidence says **real**: the
`13e408b8…` interface genuinely lists `total_supply`/`decimals`/`allowance`, so the
Fungible verdict is correct, not a mis-parse. But this task should confirm — verify
the `wasm_interface_metadata` for these intermediate hashes matches the actual
on-chain WASM exports (e.g. `stellar contract fetch` + decode), to rule out a
contract-interface extraction bug that would fabricate spurious flips.

## Implementation

- Verify the `13e408b8…` / `f35c6fb9…` interfaces against the real on-chain WASM
  (extraction correctness). If wrong → fix the interface extraction (the real bug).
- If real: in the 0320 live RMW path, when `prior_wasm_verdicts[new_hash]` differs
  from the existing `contract_type`, re-run the NFT quarantine promote/drop
  (`reclassify_contracts_from_wasm` companion in `stage.rs`): Nft → promote
  `nfts_pending`/`nft_ownership_pending`; Fungible → drop pending.
- Add the flip case to the 0320 audit-harness invariant (contract_type tracks the
  current wasm's verdict, not just the hash).

## Acceptance Criteria

- [ ] Confirmed whether the CDCN2D4O flip is real or an interface-extraction bug
- [ ] If real: upgrade that flips class re-runs quarantine promote/drop correctly
- [ ] Invariant covers contract_type, not only wasm_hash

## Widened scope (2026-09-15) — every code-derived row follows the current code

### The class of bug

We classify a contract from its code at one moment — a pool by its storage
shape at registration, a token or NFT by its interface — and write rows that
assert that identity. A WASM upgrade can later make the contract something
else. Nothing re-examines the row, so it keeps asserting an identity the
contract no longer has. It is silent: no error, no gap, a plausible row.

### The case that surfaced it

Registered config-family pool `CAZ6W4WHVGQBGURYTUOLCUOOHW6VQGAAPSPCD72VEDZMBBPY7H43AYEC`:
pool code replaced at ledger 54,515,539 (2024-11-22) by code exposing a
staking interface (`bond`, `unbond`, `distribute_rewards`); replaced again at
63,767,534 (2026-08-02) by code exposing `mint_redeem_sweep` / `sweep`, in the
transaction that moved its balances out. The pool registry and its last
reserve row still present it as a funded pool. Full timeline in task 0374,
"Production verification of the write path".

### Measurement over all upgrades (2026-09-15, prod CH)

`executable_update` topics carry the old and the new code hash
(`[sym executable_update, [Wasm, bytes old], [Wasm, bytes new]]`), so every
transition is measurable without RPC. Shape of each code from
`wasm_interface_metadata` function names — an approximation, not the
classifier: token = `balance` + `transfer` + `decimals`, NFT = `owner_of`,
pool = `get_reserves` or `query_pool_info`.

- **5,296** upgrades in the ingested range, **1,756** contracts; every code
  hash involved has an interface row (0 unknown).
- **9** upgrades changed the shape, across **6** contracts:

| Contract    | Transition                             | Misrepresented in our tables today?                     |
| ----------- | -------------------------------------- | ------------------------------------------------------- |
| `CAZ6W4WH…` | pool → other                           | **yes** — soroban pool registry                         |
| `CB7LJOYL…` | other → pool                           | no — never registered, not shown                        |
| `CDCN2D4O…` | other → token → other                  | no — net back to Other (the original case of this task) |
| `CAVK536D…` | NFT with token functions ↔ NFT without | no — still NFT (`contract_type = 2`)                    |
| `CAELDSOB…` | NFT with token functions → NFT without | no — still NFT                                          |

Scale of exposure among registered pools: 403 of 514 router-family pools and
14 of 20 config-family pools have been upgraded at least once; pair-family
pools never. A scan of all 769 registered pools' CURRENT code for their
family's pool function flags exactly one — `CAZ6W4WH…` — so the check has no
false positives on the current population.

Query shape (per partition to respect the per-query cap):

```sql
SELECT contract_id, ledger_sequence,
       hex(base64Decode(JSONExtractString(topics_xdr, 2, 'value', 2, 'value'))) AS old_hash,
       hex(base64Decode(JSONExtractString(topics_xdr, 3, 'value', 2, 'value'))) AS new_hash
FROM soroban_events
WHERE intDiv(ledger_sequence, 500000) = {P} AND signature = 'executable_update'
```

then shape both hashes through `wasm_interface_metadata`.

### Decisions (karolkow, 2026-09-15)

- **Never delete** the derived row or its history: both are true about the
  past. What changes is the claim about the present.
- **Soroban pools:** a registered pool whose current code lacks its family's
  pool interface renders as "no longer an active pool since <upgrade ledger>";
  current reserves and value are hidden, history is kept up to that ledger.
  Derivable at read time from `soroban_contracts` (current hash) +
  `wasm_interface_metadata`; no schema change, no RPC.
- **Contract type / NFT quarantine:** the original scope above stands.
- **Standing invariant:** run the transition scan each release; any new
  shape-changing upgrade of a contract that has derived rows is investigated.

## Acceptance Criteria (widened)

- [ ] Pools: the read half applies the "no longer an active pool" rule; `CAZ6W4WH…` renders accordingly
- [ ] The transition scan exists as a runnable check (runbook or harness) and is part of the release routine
- [ ] Every table holding a code-derived identity is listed with how it follows the current code (pools, `soroban_contracts.contract_type`, NFT tables, soroban `assets`)
