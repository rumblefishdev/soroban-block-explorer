# One read of a pool's own row (decided 2026-10-05)

From the architecture review (2026-09-25, re-checked on develop 2026-10-04):
the pool registry row (`liquidity_pools`) is read in 9 places with three dedup
styles — `FINAL` (`get_pool.rs:99`, `list_pools.rs:258`, `usd_analytics.rs:169`),
`ORDER BY last_updated_ledger DESC LIMIT 1` (`list_pool_activity.rs:63`,
`search/queries.rs:321`), undeduped `count()` as an existence check
(`list_participants.rs:62`) — and each per-pool endpoint has its own 404 gate.

Measured on production 2026-10-05, point lookup by `pool_id`: `FINAL` 18,158
rows / 5 ms, `LIMIT 1` 26,350 / 4 ms, `argMax` 8,192 / 4 ms; table 77,705 rows,
21,417 of 54,459 pools carry unmerged duplicates, **0** pools have two versions
at the same `last_updated_ledger` — so the three styles return identical rows
today and the cost difference is noise.

Decided:

- New `liquidity_pools/queries/pool_registry.rs`, one `fetch_pool_registry(id)`
  returning kind, legs, fee and price context, or `None`. One rule: `FINAL`
  (the table's own version column, whole row from one version).
- Replaces `pool_exists`, `fetch_pool_asset_ids`, the registry half of
  `fetch_pool_chart_context`, and search's by-id lookup. Search's text search
  over the list (`search/queries.rs:284`) stays — different shape.
  `fetch_pool_by_id` stays (it joins pool state) until the second step.
- Second step, later: latest pool state behind the same module, and the
  per-kind rules (`participant_count` null, the Soroban volume guard).
- One unmarked PR ("refactor, SQL changed"); proof: CH-gated tests + local API
  old vs new on production CH for every pool of both kinds, identical responses.
