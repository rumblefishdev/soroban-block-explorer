---
type: synthesis
status: developing
date: '2026-08-04'
spawned_from: '0463'
---

# S — where the answer comes from

Two questions need a source: **does this trustline still exist** and **what is
this account's signing configuration**. Everything below was argued out on the
measurements in `R-zero-balance-probe.md`.

## Ruled out first

**Horizon — excluded by decision, not by argument.** It is the technically
strongest fit: one `GET /accounts/{id}` returns the complete balance list
including zeros, plus signers and thresholds, and it is the only source that
can reveal the ~7 % of trustlines we hold no row for (no `getLedgerEntries`
primitive lists an account's trustlines). Karol ruled it out for the runtime
path on 2026-08-04. It stays usable as a measurement oracle. **Do not
re-propose it** without raising the exclusion itself.

**Numeric sentinel** (`amount = -1` for closed). `balance_aggregates_mv`
computes `sum(amount) AS total_supply`, so a negative sentinel silently
understates every asset's supply. A `NULL` sentinel survives the arithmetic
(`sum` and `countIf(amount > 0)` both ignore it) but needs
`Nullable(Int128)` — a column rewrite on 77 M rows, the same cost as a flag
with worse compression and a less legible meaning.

**ClickHouse `ReplacingMergeTree(ver, is_deleted)`.** Read the docs expecting
row removal; it is still a column. It needs an experimental setting to clean
up at all, "ClickHouse will keep the last row for a key even if that row is a
delete row", and `SELECT` returns deleted rows until cleanup runs. Our tables
are known not to merge down to one part (task 0420 — every read carries
`FINAL`), so correctness resting on merges is unsafe here. `DELETE` mutations
per closed trustline are also the wrong shape for a bulk-insert ingest path.

## The live options

|                                       | fixes                                             | leaves broken                                                | cost                                   |
| ------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------ | -------------------------------------- |
| **A — Soroban RPC at read time**      | zeros (93 % of the live ones) + signers           | the ~7 % we hold no row for; needs the network on a hot page | ~400 lines, 12 files, **no migration** |
| **B — `closed` column on `balances`** | zeros, permanently, offline                       | signers; entrenches the wrong model                          | schema + writer + **archive re-parse** |
| **E — trustline as an entity**        | zeros, correctly modelled; can also carry signers | balance history                                              | B's cost + new table + read repointing |
| **F — separate closures table**       | zeros, without touching the 77 M-row table        | as E                                                         | as B, minus the big-table migration    |
| **Balance-history table**             | zeros **and** charts over time                    | —                                                            | largest; its own project               |

## Why A now

**The read is lazy and the data set is not.** 33.6 M ambiguous pairs exist; a
page view asks about one to three. B/E/F precompute millions of answers so
that thousands get read.

**The history fill is the whole cost of B/E/F, and it has no cheap route.**
The RPC sweep is ~168,000 requests (above). That leaves an S3 re-parse of
13.3 M ledgers — which `docs/backfills.md` describes as a multi-machine
procedure with a manual `rsync` step and a merge its own runbook calls easy to
fumble. Nothing user-visible ships until it finishes.

**A does not block anything.** It stores nothing, versions nothing, and is
deleted when a database answer exists.

### Corrections made while arguing this out

Two claims of mine were wrong and are recorded so they are not repeated:

1. **"E replaces A entirely"** — false at the time it was said, because signers
   were thought to be unindexable.
2. **"Signers can never be complete, so the RPC call stays forever"** — also
   false, and the more important correction. Signers and thresholds live in
   `AccountEntry`, which is rewritten into the meta on **every** account change
   — a sequence bump, an incoming payment, a settings change. Our parser
   already walks those entries and simply discards the fields. It is an
   **extraction gap, not a data gap.** The only accounts that would stay
   unknown are those dormant since before ledger 50,457,424, and even for them
   the last-known signers would still be current.

So E **can** subsume A completely, signers included. The reason to do A first
is cost and laziness of the read, not capability.

## Open — deliberately not settled here

The signers/balances design is being **re-researched from scratch in a fresh
session** (Karol, 2026-08-04). Treat the A design below as the current best
answer, not as decided.

Specifically unresolved: whether the account page should make an external call
on **every** view (signers are wanted even when no zero rows exist), or fetch
them lazily behind a disclosure.
