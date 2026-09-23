-- Gate for one filled slice [{A}, {B}) — read-only (chq). Same placeholders as
-- fill_presence.sql. Prints: old_keys <TAB> new_keys. Pass: equal.
--
-- Distinct keys, not count(): both tables are version-less ReplacingMergeTrees
-- that hold unmerged duplicates, and a re-run slice adds more (50,550,000 –
-- 50,600,000: 53,879,157 rows old, 53,879,149 staging, 53,879,085 keys both).
-- The fill joins with INNER JOIN, so an old row whose transaction is missing
-- never reaches the staging table and shows up here as new_keys < old_keys; a
-- position is unique in its ledger, so two old keys can never fold into one
-- new key. Equal counts therefore mean the same set of (entity, transaction).
--
-- Counted exactly, over the key tuple, in two halves of the slice. History
-- (2026-09-23): the whole-slice tuple count ran out of the read profile's
-- 3.73 GiB at 54 M keys; a 64-bit hash of the key fitted but collided once
-- (59,350,000–59,400,000: 42,772,224 vs 42,772,225, the tuple counts equal).
-- A key includes the ledger, so the halves are disjoint and their sum is the
-- slice's count.
SELECT
    (SELECT uniqExact({K}, ledger_sequence, transaction_id) FROM {T}
     WHERE ledger_sequence >= {A} AND ledger_sequence < ({A} + {B}) / 2)
  + (SELECT uniqExact({K}, ledger_sequence, transaction_id) FROM {T}
     WHERE ledger_sequence >= ({A} + {B}) / 2 AND ledger_sequence < {B})    AS old_keys,
    (SELECT uniqExact({K}, ledger_sequence, application_order) FROM {T}_staging_position
     WHERE ledger_sequence >= {A} AND ledger_sequence < ({A} + {B}) / 2)
  + (SELECT uniqExact({K}, ledger_sequence, application_order) FROM {T}_staging_position
     WHERE ledger_sequence >= ({A} + {B}) / 2 AND ledger_sequence < {B})    AS new_keys
FORMAT TSV
