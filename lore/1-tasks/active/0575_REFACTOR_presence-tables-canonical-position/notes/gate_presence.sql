-- One count of the gate — read-only (chq). The loop in fill_presence.zsh runs
-- it for each quarter [{LO}, {HI}) of a filled slice, once on the old table
-- ({TBL} = {T}, {C} = transaction_id) and once on the staging copy
-- ({TBL} = {T}_staging_position, {C} = application_order), and compares the
-- two sums. Pass: equal.
--
-- Distinct keys, not count(): both tables are version-less ReplacingMergeTrees
-- that hold unmerged duplicates, and a re-run slice adds more (50,550,000 –
-- 50,600,000: 53,879,157 rows old, 53,879,149 staging, 53,879,085 keys both).
-- The fill joins with INNER JOIN, so an old row whose transaction is missing
-- never reaches the staging table and shows up as a short new sum; a position
-- is unique in its ledger, so two old keys can never fold into one new key.
-- Equal sums therefore mean the same set of (entity, transaction). A key holds
-- its ledger, so the quarters are disjoint and their counts add up.
--
-- Why quarters, each its own query (2026-09-23): the exact tuple count takes
-- ~100 B per key (2.31 GiB for 24 M keys), and the read profile caps a query at
-- 3.73 GiB; a whole 54 M-key slice ran out, so did four half-slice counts held
-- in one query. A 64-bit hash of the key fitted but collided once
-- (59,350,000–59,400,000: 42,772,224 vs 42,772,225 with the tuple counts equal).
SELECT uniqExact({K}, ledger_sequence, {C}) FROM {TBL}
WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}
FORMAT TSV
