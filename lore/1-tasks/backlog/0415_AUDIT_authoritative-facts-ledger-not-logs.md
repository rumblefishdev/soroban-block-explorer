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
---

# AUDIT: every authoritative fact must come from the ledger (state), not events (logs)

> **This task is the AUDIT itself — it does not fix anything directly.** Its job:
> research the whole indexer, find _every_ site where an authoritative fact is
> derived from a spoofable event/log, rank them, and **spawn a dedicated fix task
> per site** (NFT ownership is the intended first spawn — see PRIORITY TARGET). The
> "Scope" list below is the starting map, not the final list; the audit's output is
> the real list of fix tasks. Keep this task as the tracking/parent for all of them.

## Summary

The net-settled value was derived from **contract events** — which are **logs**: any
contract can emit any `"transfer"` topic it likes, so the value was spoofable. It
was rewritten to read the **authoritative ledger** (Account / Trustline /
ContractData balance changes — consensus state, unforgeable). **The same class of
bug likely exists elsewhere in the indexer**: any place we derive an authoritative
fact from an event rather than the ledger. This audit finds and ranks them.

## The rule

```
Authoritative FACT (amount moved, balance, who-holds-what, "X happened")
  → ALWAYS from the LEDGER (ledger-entry state changes: consensus, unspoofable)
Events / logs → legitimate ONLY for:
  (a) displaying them AS logs (the "events" tab — labelled contract self-report), or
  (b) a cheap candidate index ("which txs touched contract X") — but the FACT is
      then re-derived / cross-checked from the ledger.
```

A contract-emitted event is a self-report. Trusting it for a fact = trusting a log.

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
  - Events are **not part of consensus** — "not hashed into the ledger… not part of
    the protocol"; ephemeral (RPC keeps < 1 week). Only `LedgerEntryChanges` mutate
    state. Events are "the mechanism that applications off-chain can use to monitor
    movement of value" — i.e. **notifications, not truth**.
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
