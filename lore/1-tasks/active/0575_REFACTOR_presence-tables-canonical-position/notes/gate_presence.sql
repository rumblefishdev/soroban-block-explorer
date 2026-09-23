-- Gate for one filled slice [{A}, {B}) — read-only (chq). Same placeholders as
-- fill_presence.sql. Prints: old_keys <TAB> new_keys. Pass: equal.
--
-- uniqExact, not count(): both tables are version-less ReplacingMergeTrees
-- that hold unmerged duplicates, and a re-run slice adds more. The fill joins
-- with INNER JOIN, so an old row whose transaction is missing never reaches the
-- staging table and shows up here as new_keys < old_keys; a position is unique
-- in its ledger, so two old keys can never fold into one new key. Equal counts
-- therefore mean the same set of (entity, transaction).
--
-- Cost per 50,000-ledger slice, measured 2026-09-23 on 64,400,000–64,450,000
-- (old side): transaction_participants 93 M rows read / 1.5 s,
-- operation_asset_appearances 35 M / 1.0 s. A 100,000-ledger slice exceeds the
-- read profile's 3.73 GiB memory cap (uniqExact over ~72 M keys).
SELECT
    (SELECT uniqExact({K}, ledger_sequence, transaction_id) FROM {T}
     WHERE ledger_sequence >= {A} AND ledger_sequence < {B})                AS old_keys,
    (SELECT uniqExact({K}, ledger_sequence, application_order) FROM {T}_staging_position
     WHERE ledger_sequence >= {A} AND ledger_sequence < {B})                AS new_keys
FORMAT TSV
