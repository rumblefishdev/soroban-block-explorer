-- Revert lore-0209. Re-imposing NOT NULL fails if any parse_error rows
-- with NULL source_id exist; that is intentional — a manual decision
-- (delete those rows, or back-fill to a sentinel account) is required
-- before the column can go back to NOT NULL.

ALTER TABLE transactions
    ALTER COLUMN source_id SET NOT NULL;
