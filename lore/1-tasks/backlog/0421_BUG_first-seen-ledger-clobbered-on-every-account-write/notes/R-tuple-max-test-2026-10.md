# Tuple `max` as argMax on ClickHouse 26.3 (2026-10-01)

Moved out of the task README to keep it to the current state.

- **Tested on a local ClickHouse 26.3:** `SimpleAggregateFunction(max,
Tuple(Int64 ledger, value…))` acts as argMax with plain inserts — rows
  written out of order give the newest value before and after a merge; a
  newer `NULL` wins, as it should; a tie at one ledger is broken by the value
  (the writer emits one row per ledger, so it does not arise). Not tested:
  whether clickhouse-rs inserts a `Tuple` column.
