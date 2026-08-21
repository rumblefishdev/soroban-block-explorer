---
id: '0512'
title: 'FEATURE: classifier 80/20 — monitored-UNKNOWN + launchpad-NFT discriminator (drain the pending residual)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0309', '0294', '0308', '0303']
tags:
  [
    parser,
    classifier,
    nft,
    completeness,
    sep-48,
    layer-data,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0294 deep-dive on nfts_pending redundancy. The pending
      queue is NOT redundant — it is load-bearing ONLY because the WASM
      classifier is name-only and returns `Other` for bespoke NFTs. Measured
      residual is tiny (~65 contracts = ~7 WASM templates). This task is the
      actionable 80/20 increment of the strategic 0309 rebuild; the full L0-L4
      total-function (option A) stays in 0309.
---

# FEATURE: classifier 80/20 — monitored-UNKNOWN + launchpad-NFT discriminator

## Summary

`classify_contract_from_wasm_spec` (`crates/xdr-parser/src/classification.rs:101-120`)
is a name-only matcher over 5 NFT names. Bespoke NFTs whose WASM exposes none of them
classify `Other` forever, so their NFT-candidate events sit in `nfts_pending` and never
drain (the table's only remaining load-bearing reason — see auto-memory
`project-nfts-pending-load-bearing-classifier`). This task closes that gap with the
minimum change, and makes the `Other` bucket **observed** instead of silent.

## Measured residual (prod `chq`, 2026-06-23)

Distinct `nfts_pending` contracts that are `Other` WITH a WASM = **65 contracts across
only ~7 WASM templates**. They are heterogeneous:

- **Launchpad-NFT family** (custom ABI, genuine NFTs): functions `get_token_info`,
  `get_all_owned`, `get_max_token_id`, `update_token_url`, `bulk_mint`, `mint_original`,
  `is_collection_frozen`, `update_collection_info`, `freeze_collection`,
  `get_collection_info`. token_id / collection semantics. **Should classify `Nft`.**
  (auto-memory `custom_abi_nft_class_missed`.)
- **Custom financial / RWA** (NOT NFTs): functions `pay_off`, `sign_off`, `redeem`,
  `loan_status`, `set_loan_contract`, `add_vc`, `vc`, `check_paid`, `check_expired`.
  Emit `transfer`/`mint` with i128 **amounts** (not token_ids) → their pending rows are
  FALSE NFT candidates.

The other ~5,215 pending contracts (no-WASM, NULL verdict) are un-deployed SACs handled
by the task-0294 detection gate — out of scope here.

## Design (option B — the 80/20)

1. **Monitored-UNKNOWN.** Make `Other` non-silent: a `%UNKNOWN` metric + a per-WASM-template
   tripwire when a contract emits NFT-candidate events yet classifies `Other`. The pending
   table IS the unknown bucket; this only adds observability so new templates surface
   instead of rotting. (Core of 0309's "never SILENTLY miss".)
2. **Launchpad → `Nft`.** Add the launchpad discriminator names to
   `classify_contract_from_wasm_spec` so the family classifies `Nft` and `nft_reclassify`
   promotes its pending → hot. Prefer a small, named discriminator set (and consider
   signature-based matching) over a fragile single name.
3. **Custom-RWA verdict — OPEN DECISION** (how to drain their false pending):
   - (a) give them a `Fungible`/`Token` verdict (route_for → Drop) — data is amounts.
   - (b) keep `Other` but route `Other` + i128-amount-shaped data → Drop (data-type
     discriminator at NFT detection, not just WASM).
   - (c) leave `Other` → Pending but **monitored** — accept a handful of custom-RWA in the
     observed unknown bucket (API never reads `*_pending`, so harmless).
   - **Lean: (c)** — don't fabricate a verdict for contracts we don't fully understand;
     observe them. Decide at implementation.

## Out of scope

- Option **A** — the full L0-L4 total-function rebuild (SEP-48 spec-driven decode + SEP-46/47
  capability + typed-shape cascade) — stays in **0309** (effort-large, deferred; near-term
  value low since SEP-48 adoption ≈ 0). This task is the increment that captures most value now.
- Removing `nfts_pending` — premature until the residual is drained AND the classifier is
  total; see the deep-dive verdict (memory `project-nfts-pending-load-bearing-classifier`).

## Acceptance Criteria

- [ ] `Other`-with-NFT-events is observable (metric + tripwire), not silent
- [ ] Launchpad-NFT family classifies `Nft`; its pending promotes on `nft_reclassify`
- [ ] Custom-RWA pending drained or explicitly accepted as monitored-unknown (decision a/b/c)
- [ ] New classifier behavior unit-tested against the ~7 residual WASM shapes
- [ ] Docs: `classification.rs` doc updated to state `Other` is monitored, not silent

## Docs updated

- N/A at spawn time — fill `docs/architecture/xdr-parsing/*` (classifier behavior) when implemented.

## Renumbered from 0317 (2026-08-21)

This task was `0317`, which collided with the archived
`0317_BUG_contracts-events-ch-memory-limit`. The archived BUG kept the number —
it carries eight inbound references from completed tasks (0300, 0318, 0319),
and rewriting settled history is worse than renumbering an unstarted task.
Archived narrative in 0294 and 0425 still says "0317" and means _this_ task;
those files were deliberately left untouched.

## Scope extension — the parser gate (2026-08-21, from 0392)

Measured under [0392](../active/0392_BUG_nft-pending-live-routing-reconcile/README.md);
full evidence lives there. This task previously covered only the _classifier_
gate — contracts stamped `Other` that should be `Nft`. It now also covers the
_parser_ gate, because they are the same defect seen twice and splitting them
leaves one half ownerless. The task 0309 nominates for this work, `0308`, does
not exist — its link dangles.

### The parser gate loses more than the classifier gate quarantines

|                                                 | Contracts | Events  |
| ----------------------------------------------- | --------- | ------- |
| decisive `Nft` verdict, rows present            | 67        | 25,110  |
| decisive `Nft` verdict, **rows nowhere at all** | **66**    | **692** |

The second row is not quarantined — it is absent. No hot row, no ownership row,
no pending row, no log line. The classifier said `Nft`; the parser could not
shape the events; nothing recorded the drop.

### Cause 1 — argument shape, not event name

`stellar contract info interface --network mainnet` against the deployed WASM:

- `CB2SIYGHFGQMKEYQUWCTF3HCWBCPFUSRGVWXOPV3LIJR7K5LRPFXZEYK` —
  `transfer(env, domain: String, from: Address, to: Address)`,
  `owner_of(domain: String)`. Token identity is a **String**, in **first**
  position; canonical is `(from, to, token_id)`. This contract emits a
  perfectly canonical `transfer` event name, so name-based matching alone would
  not have rescued it. A name-only discriminator is therefore insufficient for
  this task's goal.
- `CBT5JMDOUAU3BJF7YZR42LVODLMZSQE4LIJUJNUBKEC2VZOXIF4JFBRU` —
  `owner_of(token_id: u32)`, `transfer`, `mint_badge`, `total_supply`.
  Unambiguously an NFT; mints via `mint_badge`, emits `init` / `minted`.

### Cause 2 — signature extraction is position-0-Symbol-only

`extract_event_signature` (`crates/db-clickhouse/src/persist/stage.rs:2137`)
returns `None` unless `topics[0].type == "sym"`. Two live shapes defeat it, and
they account for the single largest slice of orphan events — **470 events across
5 contracts** with a NULL `signature`:

- **namespace-first**: `["BadgeNFT", sym:"init"]`, `["StoryNFT", sym:"init"]`,
  `["identizy_nft", "minted"]` — a String collection name occupies topic 0 and
  the event name sits at topic 1.
- **String-typed event name**: `["Mint"]` as a String rather than a Symbol —
  168 events on one contract.

Neither is malformed. Both are invisible to us by construction, and because the
signature is NULL rather than wrong, they are also invisible to any monitoring
keyed on event name.

Other orphan signatures by volume: `minted` 63, `uri_upd` 58, `mint` 46, plus
`identity_minted`, `mint_event`, `transfer_event`, `set_uri_event`,
`approval_for_transfer_event`.

### Residual re-measured — the 80/20 still holds, but wider

The quarantined population is now **67 contracts across 18 distinct WASM
templates** (this task measured ~7 templates / 65 contracts on 2026-06-23 — the
contract count barely moved, the template spread nearly tripled). Concentration
still favours the 80/20:

| WASM prefix    | Contracts |
| -------------- | --------- |
| `DEFA6F74E04B` | 18        |
| `D26B246D23E8` | 13        |
| `E84BE55DCA2C` | 11        |
| `67848B7AB5A3` | 6         |

Four templates cover **48 of 67 contracts (72%)**.

### Consequence for 0392

Every contract this task promotes `Other → Nft` turns its quarantined rows into
resolved-but-stranded rows, which nothing currently promotes. 0392 owns that
mechanism and is sequenced immediately after this task. Landing this one without
0392 replaces a silent-miss defect with a stale-data defect.

### Prior art worth recovering before implementing

Task `0308` (custom-ABI NFT parser + classifier coverage) held an exhaustive
**NFT event-shape catalog** — SEP-50 + OpenZeppelin documented shapes, the
NFT-vs-SEP-41 discriminator, four verified on-chain DATA encodings, and the
custom/undocumented tail. 0308 was abandoned by decision on 2026-06-22
(`f92ce582`, code and worktree deleted with it), but the catalog is intact in
git history — 110 lines:

```bash
git show ec160781:lore/1-tasks/active/0308_FEATURE_custom-abi-nft-parser-classifier-coverage/notes/R-nft-event-shape-catalog.md
```

Read it before designing the discriminator. Do not resurrect 0308 itself — the
abandonment was deliberate and this task carries the scope now.
