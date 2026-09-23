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
-- Keys are counted as a 64-bit hash: uniqExact over the 3-column tuple ran out
-- of the read profile's 3.73 GiB on a 54 M-key slice (2026-09-23); over the
-- hash it takes 1.3–1.7 s there. A hash collision (~1e-4 per slice) can only
-- lower one side's count, so it fails the gate rather than passing a bad slice.
SELECT
    (SELECT uniqExact(cityHash64({K}, ledger_sequence, transaction_id)) FROM {T}
     WHERE ledger_sequence >= {A} AND ledger_sequence < {B})                AS old_keys,
    (SELECT uniqExact(cityHash64({K}, ledger_sequence, application_order)) FROM {T}_staging_position
     WHERE ledger_sequence >= {A} AND ledger_sequence < {B})                AS new_keys
FORMAT TSV
