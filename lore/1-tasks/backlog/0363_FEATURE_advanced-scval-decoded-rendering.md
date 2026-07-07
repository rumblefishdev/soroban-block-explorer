---
id: '0363'
title: 'FEATURE: advanced tx-detail — decode ScVals to typed chips + collapse (kill the raw-JSON wall)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0352', '0071']
tags:
  [frontend, transaction-detail, soroban, ux, priority-medium, effort-medium]
links: []
history:
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Advanced-mode tx detail dumps raw ScVal JSON ({type,value}) for event
      topics/data and operation args — a ~13,000px wall. Audited our components
      + researched how mature explorers do it (stellar.expert = same-chain
      reference, Etherscan/Solscan). Everyone decodes to typed/human; raw is
      secondary. We're the outlier. This task moves us to decoded rendering.
      Shares the ScVal decoder with 0352 (error chip) — do together.
---

# FEATURE: decoded ScVal rendering in advanced tx detail

## Summary

The advanced transaction-detail view renders Soroban event **topics/data** and
operation **arguments** as raw, pretty-printed ScVal JSON (`{type,value}` trees)
via `HighlightedJson`. A tx with ~30 events becomes a ~13,000px scroll wall where
the meaning is buried under JSON scaffolding. The input is already a **typed
discriminated union** (`{type, value}`) — so a type-aware renderer is
straightforward. Adopt the pattern every mature explorer uses: **decoded typed
chips as the primary view, raw behind a toggle/collapse.**

## Current state (what we have vs the options)

Options considered (A ⊂ B ⊂ C; D=lib, E=raw):

| Option                                    | Have it?                 | Evidence                                                                                                                                                                                                                                          |
| ----------------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **E — raw JSON** _(baseline)_             | ✅ this is us now        | Events `topics`+`data`, ops `args/return/auth` → `HighlightedJson` = blind `{type,value}` dump (`web/src/pages/transaction-detail/advanced/HighlightedJson.tsx`).                                                                                 |
| **A — collapse / progressive disclosure** | 🟡 partial, wrong place  | ✅ raw XDR section (`XdrRow.tsx`, collapsed+expand+copy). ❌ NOT on Events/Operations — always fully expanded (the wall). Pattern exists, not applied where it hurts.                                                                             |
| **B — semantic ScValView**                | 🟡 fragments only (~10%) | `inlineScalar` (bool/number/short-string → Chip) in `OperationJsonDetail.tsx`; `IdentifierDisplay` only on the Contract column; `categoryChip` Soroban/Classic. No `switch(.type)` renderer — addresses/amounts INSIDE topics/data/args stay raw. |
| **C — "assets moved" / event decode**     | ❌ no                    | —                                                                                                                                                                                                                                                 |
| **D — JSON-viewer lib**                   | ❌ no                    | `HighlightedJson` hand-rolled.                                                                                                                                                                                                                    |

**So: E + a slice of A (raw-XDR only) + scattered B.** Target = **A + B + C-lite.**

## Reference — how mature explorers do it (researched)

- **stellar.expert (same chain, open-source — north star):** ScVals rendered as
  **typed chips with a small dimmed type subscript** (`0i128`, `164266u32`,
  `"mint"sym`, `…bytes`); maps/vecs as `{key: val}` / `[…]`. Invocation shown as
  a **function signature `fn(arg,arg) → result`** with a **nested sub-call tree**
  (each sub-call's return after `→`); an ⓘ tooltip fetches the WASM interface to
  show **named parameters**. Events decoded **twice**: a semantic line
  (`0.55 KALE minted`) + the decoded `["mint"sym, …] with data …i128` (topics vs
  data separated). Zero raw XDR embedded — links out to Stellar Lab/Horizon for
  raw. Addresses truncated + identicon + clickable + known-name label (`[Kale]`).
- **Etherscan / Solscan / Blockscout:** decoded is the **default**, raw/hex is a
  toggle or a tab labeled "debugging"; a plain-language **"assets moved" summary
  line** at the top (Tokens Transferred / Balance Change), decimal-adjusted +
  logo + truncated links; typed **named param tables** (`#/Name/Type/Value`) with
  the resolved signature as header; **prominent decoded failure reason** in the
  status row (red "Fail with error 'X'"); verbose logs collapsed by default.
- **Architectural note:** StellarChain's invocation trace uses **ephemeral
  Soroban RPC → disappears for old txs.** stellar.expert (and us, via `heavy`
  archive-XDR fetch, ADR 0029) reconstruct from **persisted** meta → always
  available. Our foundation is on the robust side; keep it.

## Target design (A + B + C-lite)

### B (core) — `<ScValView value>` component

A recursive, type-aware renderer replacing `HighlightedJson` wherever a ScVal
enters. Our decoder emits these `type`s: `sym, string, address, bool, bytes,
i128/u128/i64/u64/i32/u32, timepoint, duration, vec, map, error, void,
contract_instance, ledger_key_nonce, ledger_key_contract_instance`.

| ScVal type                  | Render                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------- |
| `sym`, `string`             | text (event `topics[0]` sym → name badge)                                             |
| `address`                   | `IdentifierDisplay` (truncate 4…4, clickable; account vs contract by G/C)             |
| `i128/u128/i64/u64/i32/u32` | mono number, grouped / `formatCompactAmount`, full in tooltip, small `type` subscript |
| `bool`                      | `Chip`                                                                                |
| `bytes`/hashes              | mono truncated 4…4 + copy                                                             |
| `timepoint`/`duration`      | human time / duration                                                                 |
| `error`                     | error chip `Type/Code` — **shared with 0352**                                         |
| `void`                      | `—`                                                                                   |
| `vec`                       | scalars → inline `a, b, c`; structured → nested rows (collapse)                       |
| `map`                       | `key → value` pairs                                                                   |

Keep a per-node/section **"raw"** affordance that falls back to today's
`HighlightedJson` (devs keep the raw).

### A — collapse Events/Operations

Wrap each event / operation in the `XdrRow`-style collapsed row: decoded summary
line visible; expand for the full decoded detail; raw JSON one more click in.
Default collapsed for large sets (mirror stellar.expert's "Show operation
details" / N-more spoiler).

### C-lite — semantic layer

- **Events:** for known shapes (`transfer/mint/burn/approve`, SAC) render a
  one-line summary `Transfer 326,131,711 · GX…4P → GY…8K` above the decoded
  topics/data.
- **Invocation:** render `fn(args) → result` signature + nested sub-call tree
  with return values, instead of a flat `arguments` blob.
- **Named params (bonus):** we ingest ABIs (`wasm_interface_metadata`) → map
  positional args to `#[contractfn]` param names (stellar.expert's ⓘ pattern).

## Wireframe (Events)

Now:

```
Type      Topics                        Data
Contract  [ {"type":"sym",              { "type":"i128",
             "value":"fee"}, … ]          "value":"947930" }
```

Target (collapsed):

```
● transfer   CCW67T…MI75   GX…4P → GY…8K        326,131,711   [raw ⌄]
● fee        CAS3J7…OWMA   GB6KVOP2…KNFCP            947,930   [raw ⌄]
```

## Component inventory (reuse, don't rebuild)

`IdentifierDisplay` (addresses), `Chip` (bool/badge/event-name), `formatCompactAmount`
(amounts), collapse pattern from `XdrRow`, `useCopyToClipboard`, and fold in the
existing `inlineScalar` logic. **New: only `ScValView`.**

## Scope / cost

- **Pure frontend.** Data already typed via `heavy` (archive XDR) — **zero
  backend, zero indexing.**
- Core = 1 component (`ScValView`) + wiring in `EventsSection` + `OperationJsonDetail`.
- **Do with 0352:** the `error` ScVal → typed chip is the same decoder as the
  fail-reason banner. `scval.rs:19` currently drops the code — fix there feeds both.

## Implementation Plan

1. `ScValView` — recursive `switch(value.type)` renderer (table above), with a raw fallback.
2. Wire into `EventsSection` (topics/data) and `OperationJsonDetail` (args/return/auth); retire `inlineScalar` into `ScValView`.
3. Collapse each event/operation (A) — `XdrRow`-style, decoded summary visible, raw one click deeper.
4. C-lite: known-event semantic line + invocation signature/call-tree.
5. (bonus) named params from `wasm_interface_metadata`.
6. Keep `RawDataSection` (base64 XDR) collapsed; optionally add an "open in Stellar Lab" deep-link.

## Acceptance Criteria

- [ ] Event topics/data + operation args render as typed chips (addresses linked+truncated, amounts formatted), not raw `{type,value}` JSON
- [ ] Events/Operations collapsed for large sets; raw JSON still reachable per node/section
- [ ] Known events (transfer/mint/burn) show a one-line semantic summary
- [ ] Invocation shown as `fn(args) → result` with sub-call tree
- [ ] `error` ScVal renders as `Type/Code` chip (shared with 0352)
- [ ] Page height for a ~30-event tx drops from ~13k px to a scannable list
- [ ] **Docs updated** — N/A (pure FE presentation)
- [ ] **API types regenerated** — N/A unless `scval.rs` error shape changes (then coordinate with 0352 + regen)

## Notes

- Related: 0071 (original advanced tx-detail), 0352 (fail-reason banner — shared decoder), 0013 (shared xdr/scval parsing).
- Research artifact from the explorer comparison: `stellarchain-tx.jpeg` (worktree, disposable).
