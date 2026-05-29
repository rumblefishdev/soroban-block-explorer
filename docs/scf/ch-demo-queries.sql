-- =====================================================================
-- ClickHouse demo queries for the SCF Milestone 1 submission video
-- (Scene 6 of milestone_one_video_scenario/m1-scenario.md).
--
-- Database:  default      (the in-container CH default DB)
-- Tables:    ledgers, soroban_events, soroban_contracts, transactions
--
-- HOW TO RUN — three options, pick whichever is most camera-friendly:
--
-- 1) Interactive REPL (recommended for the demo recording):
--      ./ch-demo-run.sh        # opens clickhouse-client REPL in container
--    Then paste one query at a time; output renders as a tidy table.
--
-- 2) One-shot batch run of THIS file (good for a smoke check):
--      ./ch-demo-run.sh --file ch-demo-queries.sql
--
-- 3) Manual (no helper script) — from /srv/app on the Hetzner box:
--      sudo docker compose -f /srv/app/docker-compose.yml \
--                          -f /srv/app/docker-compose.prod.yml \
--                          exec clickhouse \
--          clickhouse-client \
--              --config-file=/etc/clickhouse-backup/client.xml \
--              --multiquery
--
-- Demo target (pre-picked from empirical discovery on 2026-05-26):
--   contract_id  CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK
--                = Aquarius (AquaToken) Liquidity-Pool Router
--                  — verified on stellar.expert, multi-hop swap router,
--                    1.97M events lifetime, 26 K swap events / 15 K txs
--                    in the last 100 k ledgers.
--   tx_hash      6cad2d49962ae5962722f1f90d4fd11f9e04bd644ad4873752ae1416fddd4740
--                = recent Aquarius Router tx with 4 swap events
--                  (multi-hop), ledger 62,527,948.
-- =====================================================================


-- ─── Query 1 — Coverage ────────────────────────────────────────────────
-- How many distinct ledgers we hold, and the sequence range we cover.
-- The lowest sequence is the start of the historical backfill (Soroban
-- mainnet activation); the highest is the current network tip.
-- ----------------------------------------------------------------------

SELECT count(DISTINCT sequence) AS distinct_ledgers,
       min(sequence)            AS first_ledger,
       max(sequence)            AS tip
FROM ledgers
FORMAT PrettyCompactMonoBlock;


-- ─── Query 2 — Completeness (the Milestone-1 proof) ───────────────────
-- expected_span − distinct_ledgers must equal 0. That is "no gaps from
-- backfill start through tip" — the literal AC #2 wording.
-- ----------------------------------------------------------------------

SELECT (max(sequence) - min(sequence) + 1) AS expected_span,
       count(DISTINCT sequence)            AS distinct_ledgers,
       expected_span - distinct_ledgers    AS missing
FROM ledgers
FORMAT PrettyCompactMonoBlock;


-- ─── Query 3 — Live tail ──────────────────────────────────────────────
-- Run, wait ~10 s on camera, run again. The top `sequence` advances —
-- proof that the live indexer is keeping up with mainnet in real time.
-- ----------------------------------------------------------------------

SELECT sequence, closed_at, transaction_count
FROM ledgers
ORDER BY sequence DESC
LIMIT 10
FORMAT PrettyCompactMonoBlock;


-- ─── Query 4 — Full-content CAP-67 events for a known DEX contract ────
-- Contract is the pre-picked Aquarius soroswap-style router (see header).
-- Returns one row per event, with `topics_xdr` and `data_xdr` already
-- ScVal-decoded to readable JSON (the column names keep an historical
-- `_xdr` suffix but the codec comment in init.sql confirms decoded
-- content).
--
-- The StrKey → surrogate Int64 resolution is done via scalar subquery
-- (not INNER JOIN) so the resolved constant can be propagated into the
-- soroban_events PK skip index (contract_id, ledger_sequence, …); with
-- optimize_read_in_order = 1 the LIMIT 20 reverse-scan stops as soon
-- as 20 rows are produced. INNER JOIN form blocks that pushdown.
-- Empirical: ~21 ms vs ~4 s for the JOIN form.
-- ----------------------------------------------------------------------

SELECT e.ledger_sequence,
       e.event_index,
       e.event_type,
       e.signature,
       e.topics_xdr,
       e.data_xdr
FROM   soroban_events AS e
WHERE  e.contract_id = (
           SELECT id
           FROM   soroban_contracts FINAL
           WHERE  contract_id = 'CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK'
           LIMIT  1
       )
ORDER  BY e.ledger_sequence DESC
LIMIT  20
SETTINGS optimize_read_in_order = 1
FORMAT Vertical;


-- ─── Query 4b — Same events, spot-checked BY TRANSACTION HASH ─────────
-- Matches the AC #3 wording exactly ("spot-checked by transaction
-- hashes"). The transaction is the pre-picked multi-hop Aquarius router
-- trade (see header).
--
-- Uses the dedicated `transaction_hash_index` table for a PK lookup on
-- `hash` (ORDER BY hash → μs), then propagates the resolved
-- `ledger_sequence` as a scalar constant. That partition-prunes
-- `soroban_events` (`PARTITION BY intDiv(ledger_sequence, 500000)`) to
-- a single 500k-ledger part, and `t.hash = unhex(...)` (raw
-- FixedString(32) compare) activates the bloom-filter index
-- `idx_tx_hash_bloom` on `transactions.hash`. A `lower(hex(t.hash))`
-- form would defeat that bloom filter because the function call on the
-- indexed column blocks index use. Empirical: ~66 ms vs ~88 s for the
-- `lower(hex())` form.
-- ----------------------------------------------------------------------

WITH tx AS (
    SELECT ledger_sequence
    FROM   transaction_hash_index FINAL
    WHERE  hash = unhex('6cad2d49962ae5962722f1f90d4fd11f9e04bd644ad4873752ae1416fddd4740')
    LIMIT  1
)
SELECT e.event_index,
       e.event_type,
       e.signature,
       e.topics_xdr,
       e.data_xdr
FROM   soroban_events AS e
INNER  JOIN transactions AS t
       ON  t.id              = e.transaction_id
       AND t.ledger_sequence = e.ledger_sequence
WHERE  e.ledger_sequence = (SELECT ledger_sequence FROM tx)
  AND  t.hash            = unhex('6cad2d49962ae5962722f1f90d4fd11f9e04bd644ad4873752ae1416fddd4740')
ORDER  BY e.event_index
FORMAT Vertical;
