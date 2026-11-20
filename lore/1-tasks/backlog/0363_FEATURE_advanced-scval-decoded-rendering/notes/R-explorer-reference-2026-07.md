---
title: 'How mature explorers render ScVals (researched 2026-07)'
type: research
status: mature
spawns: []
tags: [frontend, transaction-detail, peers]
links: []
history:
  - date: '2026-11-17'
    status: mature
    who: karolkow
    note: 'Moved out of the task README unchanged, to make room for the premise review.'
---

# Reference — how mature explorers do it (researched)

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
