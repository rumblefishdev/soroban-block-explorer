# R — W1 amounts vs `asset_transfers` (2026-09-30)

Decisions 152, 154 A, 157 B.

Could a movement's amounts come from `asset_transfers` instead of parsing
each family's pool event? Measured, read-only: no. The pool event is needed
for the kind, for grouping, and for tokens `asset_transfers` never sees (a leg
token emitting `transfer_event` instead of SEP-41 `transfer` — 616 of 120k
Aquarius constant swaps in 200k ledgers). Config (Phoenix) swaps differ by the
commission paid to a third party (the event is the trader's view). Kept as an
independent witness instead: `pool_movements_vs_asset_transfers` (#540)
compares, per operation with one amount event and no payout, the transfers
touching the pool with the decoded amounts; named classes: non-standard
token, commission, balance surplus judged by the pool's own reserves (an
exhausted swap keeps its whole input; a router sends a token beyond the
trade; a pair counts dust as input). Green on six slices, 55M to the tip; pair
100%; ~2% not compared. Also in #540: pool movements are staged inside
`prepare` (StageInputs carries the registry), no post-prepare mutation.
