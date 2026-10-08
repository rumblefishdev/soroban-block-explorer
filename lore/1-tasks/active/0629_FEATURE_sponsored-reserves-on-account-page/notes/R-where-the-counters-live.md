# R: where the sponsorship counters live

Moved from the task README on 2026-10-08 (stage 1 research).

**Where the numbers live.** `AccountEntry.ext.v1.ext.v2` carries
`num_sponsoring` (reserves this account pays for others) and `num_sponsored`
(reserves of this account paid by others). They change only together with the
`AccountEntry`, which the live writer already turns into an
`account_entry_state` row (`persist/stage.rs`, whole-row per observed entry),
and which the checkpoint seed already covers for accounts older than our
ledger floor (`backfill-runner/src/snapshot/entry_state.rs`, task 0521). Today
both counters are read past and dropped.

**Why the counter and not a count of rows.** One sponsored account costs 2
reserves, a claimable balance one per claimant; the counter is what the
network enforces and what stellar.expert shows. A list of sponsored entries
(stage 2) would not sum to it row by row.

**Who is sponsoring, measured 2026-10-06.** Last 7 days on production:
69,266 `BEGIN_SPONSORING_FUTURE_RESERVES` operations from 100 source accounts,
56 `REVOKE_SPONSORSHIP` from 4. Read live via RPC `getLedgerEntries`:
`GAUA7XL5…PNJU` sponsors 4,043,490 reserves, `GDB3RSSW…6CU` 111,104 — so a
list must be paginated and keyed by sponsor.

**Trap for the refill.** The seed wrote an `account_entry_state` row only
when the network's entry was newer than our newest row (task 0521), so rows
written before the columns existed — current, only incomplete — were never
rewritten. Decided 2026-10-07: the seed writes every live account at the
entry's own ledger and the version rule decides (equal version: the seed's
later insert wins; newer live write: ours wins). Rejected: a refill flag with
a hand-typed ledger floor — more code, a number to get wrong, and the next
new column would need it again. Given up: 0521's "repeat pass writes ~0"
signal.
