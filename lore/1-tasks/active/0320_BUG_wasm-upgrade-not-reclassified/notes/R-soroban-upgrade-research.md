---
title: 'R — Soroban WASM upgrade & immutability: docs + mainnet research'
type: research
status: mature
spawned_from: README.md
spawns: []
tags: [soroban, wasm-upgrade, cap-0046, mainnet, executable_update]
links:
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-02.md
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-05.md
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-12.md
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0066.md
  - https://developers.stellar.org/docs/build/guides/conventions/upgrading-contracts
history:
  - date: 2026-06-24
    status: mature
    who: karolkow
    note: >
      Deep-research session with Claude. Official docs (101-agent fan-out,
      3-0 adversarial verify) + independent mainnet check (prod CH via chq,
      Soroban RPC ground-truth, stellar.expert). Repo + task spec treated as
      claims to verify, not truth.
---

# R — Soroban WASM upgrade & immutability (docs + mainnet)

> Research note, 2026-06-24, karolkow + Claude. Status: mature.
> Method: official CAP/SDK docs (primary, adversarially verified) cross-checked
> against mainnet — every chain claim confirmed by ≥2 independent sources
> (prod ClickHouse, Soroban RPC `stellar contract fetch` sha256, stellar.expert).

## Question

Can a deployed Soroban contract change its WASM after deploy? By what mechanism?
Same contract_id or new instance? How common on mainnet? Is immutability an
on-ledger flag? Can an upgrade change a contract's class (NFT↔token↔other)?

## Docs findings (primary-source, 3-0 verified)

| #   | Fact                                                                                                                                                                                                                                                                           | Source                   |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------ |
| 1   | Upgrade = `update_current_contract_wasm(new_hash)`, called by the contract on itself; new WASM must be **pre-uploaded**; swap applies after the invocation. SACs cannot (no WASM executable).                                                                                  | CAP-0046-02              |
| 2   | **Same contract_id** — wasm_hash is not an input to address derivation. No "new instance" on upgrade.                                                                                                                                                                          | CAP-0046 + upgrade guide |
| 3   | Upgrade mutates `executable.wasm_hash` **in place** in the single CONTRACT_DATA instance entry (`SCV_LEDGER_KEY_CONTRACT_INSTANCE`) and emits SYSTEM event `["executable_update", old, new]`. `SCContractInstance` = `{executable, storage}` only — **no immutability field**. | CAP-0046-05 + XDR        |
| 4   | **No on-ledger immutability flag.** The host fn is unconditionally available; "non-upgradeable" = the code never exposes an upgrade path. Detectable only heuristically. ("renounce-owner ⇒ immutable" refuted 0-3.)                                                           | CAP-0046-02, OZ          |
| 5   | Interface **can** change across upgrade (NFT-like ↔ token-like possible) → classification is time-varying per contract_id.                                                                                                                                                     | SEP-0049, upgrade guide  |
| 6   | TTL archival/restore reactivates the **same** entry without changing contract_id or wasm_hash (P23 auto-restore). `executable_update` only fires on a real executable change.                                                                                                  | CAP-0046-12, CAP-0066    |

**Could NOT confirm** (so the plan does not depend on them): the exact
`LedgerEntryChange` variant on upgrade (single `updated` vs `state`+`updated`
pair) — the one live-tx claim was refuted 0-3. SAC ledger-level discriminator
(`CONTRACT_EXECUTABLE_TOKEN`) refuted 1-2.

## Mainnet evidence (measured 2026-06-24, prod CH + RPC)

- **Prevalence: 1,362 contracts upgraded, 4,691 `executable_update` events, all non-SAC.**
  311k/424k contracts are SACs (can't upgrade). Upgrades concentrate in active
  protocols: 0/16 random non-SAC upgraded, 3/4 busiest did.
- **Bug confirmed by RPC ground-truth** (not just our DB). e.g. `CCABO2IQ…`:
  deploy wasm `8b89f74f` (our CH) → live chain `57e51099` (RPC `stellar contract
fetch` sha256). The `executable_update` event chain reconstructs deploy→current
  exactly: first event OLD = `8b89f74f`, last event NEW = `57e51099`, 5 events =
  stellar.expert "6 versions".
- **`executable_update` already ingested** in `soroban_events` (signature column),
  topics = decoded JSON `[Symbol("executable_update"), [Symbol("Wasm"), old], [Symbol("Wasm"), new]]`.
  Latest event's NEW-hash = live executable: **validated 28/28 vs RPC**.
- **0 of 1,362** current wasms missing from `wasm_interface_metadata` → no
  reclassification regression risk.

## Class-change analysis (the scope-defining result)

Classified every wasm in the upgrade chains with the production rule
(`classify_contract_from_wasm_spec`: NFT names → Nft; else decimals/allowance/
total_supply → Fungible; else Other), computed in ClickHouse (full coverage):

- **Per-transition (n=4,691): exactly 2 class changes** — `Other→Fungible` ×1 and
  `Fungible→Other` ×1, **both on the same contract** `CDCN2D4OF5IHPAHUIF6RPVH654KW6LKTYKYK3IQULBBWURD7L4CDNSRO`
  (upgraded 37×; briefly exposed a fungible interface over 4 ledgers, then reverted).
- **Per-contract net (deploy→current, n=1,362): 0 class changes.** Every contract
  is the same class now as at deploy (922 Other, 426 Fungible, 14 Nft).
- **0 NFT flips ever.** Nothing became/stopped being an NFT or an asset.

**Implication:** at current state the fix is "update the `wasm_hash` field" for
1,362/1,362 contracts. The NFT quarantine promote/drop path is never exercised by
real mainnet data — handle it defensively, but it is not a live-correctness issue.

## Tooling (reproducible)

- `chq "SQL"` — prod ClickHouse. Key tables: `soroban_contracts` (RMT, current
  pointer), `wasm_interface_metadata` (hash→interface, append-only), `soroban_events`
  (`signature='executable_update'`).
- `stellar contract fetch --id C… --rpc-url https://mainnet.sorobanrpc.com
--network-passphrase "Public Global Stellar Network ; September 2015"` → sha256 =
  current on-chain wasm hash (ground truth).
- stellar.expert `/explorer/public/contract/{id}` → `versions` field = #wasm versions.
