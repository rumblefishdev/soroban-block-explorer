-- Reverse of 20260507000000_operations_appearances_application_order.up.sql.
-- Drops the CHECK constraint and the column.

ALTER TABLE operations_appearances
    DROP CONSTRAINT IF EXISTS ck_ops_app_application_order_range;

ALTER TABLE operations_appearances
    DROP COLUMN IF EXISTS application_order;
