# R: implementation plan, both stages

Moved from the task README on 2026-10-08, when the task closed.

Each PR is one production step. Stage 2 starts only after stage 1 ships.

### Stage 1 — counts (certain: the data and both writers exist)

| PR  | Scope                                                                                                                                         | Production acts after merge                        |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| 1a  | Parser reads `num_sponsoring` / `num_sponsored`; `account_entry_state` gains two columns (`DEFAULT 0`); live writer and seed fill them — #626 | `ALTER TABLE … ADD COLUMN` ×2, then indexer deploy |
| 1b  | Seed writes `account_entry_state` for every live account (drops task 0521's narrowing) — #638                                                 | `snapshot-seed --execute`; check counts vs RPC     |
| 1c  | API `account` detail exposes both counts; the page shows them (two rows in Summary, prototype variant A)                                      | API + SPA deploy                                   |

1c waits for the seed run after #638 (its `same ledger with other counters`
count is the repair) and for a stratified RPC sample (top
sponsors, sponsored accounts, an account with neither, one last changed
before our floor) to match. A refilled row keeps its old version by design,
so "no row older than 1a" can never be the gate. Shipped earlier, the page
would show 0 where the chain says otherwise — a wrong number, not a missing
one. An
account with no `account_entry_state` row shows "unknown", as Signers does.

**Rejected: reading the counters live via RPC on each page load** — cheaper
(no ALTER, no refill), but every account page would then depend on an external
RPC at request time, a second source of truth beside the index, and stage 2
needs the stored data anyway.

### Stage 2 — who pays each sponsored entry (decided 2026-10-07)

Shaped like stellar.expert, which lists sponsors on the **sponsored**
account ("Account base reserve sponsored by X", "USDC trustline sponsored by
X") and shows only a count on the sponsor's side. Read **live from RPC**, not
indexed: `getLedgerEntries` returns each entry's `sponsoring_id` in `extXdr`,
the account's `signer_sponsoring_ids` cover signers, and our `balances` name
the trustlines to ask for. PR 2a (#646) shares the RPC pool of the WASM
fetcher; PR 2b (#647) adds `GET /v1/accounts/{id}/sponsorship` and a
"Sponsored reserves" card grouped by sponsor. **Not delivered:** a list on
the sponsor's side ("whom GAUA7… pays for") — it needs an index over the
whole network (4M entries for one wallet), stellar.expert does not offer
it, and nobody asked again; dropped, not deferred.

Re-checked 2026-10-08: Horizon answers it (`accounts?sponsor=X`, 200
accounts a page in ~1.9 s, sponsor per trustline and signer, none per data
entry, no total). Declined: it would make the page depend on a public
service SDF is winding down (no new features, RPC preferred), behind a rate
limit our Lambdas share through AWS egress. If asked for, build our own
index instead.
