---
id: '0243'
title: 'FEATURE: API feature flag per module — gradual PG↔CH migration for all 9 handler modules'
type: FEATURE
status: backlog
related_adr: ['0044', '0046']
related_tasks: ['0207', '0228', '0239', '0240', '0241', '0244']
blocked_by: ['0241', '0239', '0240']
tags:
  [
    priority-high,
    effort-large,
    layer-api,
    layer-backend,
    clickhouse,
    feature-flag,
    gradual-migration,
  ]
milestone: 2
links:
  - crates/api/Cargo.toml
  - docs/architecture/database-schema/endpoint-queries/
  - lore/1-tasks/archive/0207_FEATURE_clickhouse-endpoint-queries-reference-set.md
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned z M1-M3 sequencing planu (2026-05-20). Zamyka lukę D2 AC #1
      ("all REST API endpoints schema-valid against mainnet data") po pivocie
      PG→CH. Aktualnie crates/api/ używa wyłącznie sqlx (PG). Decision team
      (2026-05-20): feature flag per module (9 flags, gradual rollout, safe
      rollback per handler).
---

# API feature flag per module — gradual PG↔CH migration

## Summary

`crates/api/` używa wyłącznie `sqlx` (Postgres). Po deploy 0241 (indexer hard
swap CH-only), PG nie dostaje nowych ledgerów → API zwraca stale data. Task
przepisuje API z PG na CH per moduł z feature flag (9 flags = 9 modułów),
pozwalając na gradual rollout i safe rollback per handler.

## Context

- 9 handler modułów w `crates/api/src/`: accounts, assets, contracts, ledgers,
  liquidity_pools, nfts, network, search, transactions (23 endpoints total).
- Każdy ma per-module `queries.rs` z `sqlx::query!`.
- 17 reference CH SQL queries już zmapowane w
  `docs/architecture/database-schema/endpoint-queries/01..17_*.sql` (task 0207).
- 87 existing integration tests w `tests_integration.rs`.

D1 = write-path correctness (M1 task 0241). D2 = read-path correctness (this task).

## Implementation Plan

### Step 1: Cargo deps + connection config

`crates/api/Cargo.toml`:

```toml
db-clickhouse = { path = "../db-clickhouse" }
clickhouse = { workspace = true }
# Keep sqlx for transition; task 0244 removes it post-stable
```

Connection pool init at Lambda cold-start (`clickhouse::Client::default().with_url(...)`).
mTLS config: cert + key + ca z Secrets Manager (per 0239 Phase 2, user
`api_reader` per 0240).

### Step 2: DataSource enum + dispatch

```rust
// crates/api/src/common/datasource.rs
pub enum DataSource {
    Pg,
    Ch,
}

impl DataSource {
    pub fn for_module(module: &str) -> Self {
        match std::env::var(format!("API_DATASOURCE_{}", module.to_uppercase())).as_deref() {
            Ok("ch") => Self::Ch,
            _ => Self::Pg, // default during transition
        }
    }
}
```

### Step 3: Per-module migration (9 modułów)

Dla każdego z 9 modułów:

1. Dodać `queries_ch.rs` z CH equivalent dla każdego `queries.rs::*` function.
   Reuse SQL z `docs/architecture/database-schema/endpoint-queries/`.
2. W handlerze: dispatch `match DataSource::for_module("...") { Pg => queries::*, Ch => queries_ch::* }`.
3. Schema parity: response shape identyczny — zachować typed structs, mapowanie
   `clickhouse::Row` → existing `domain::*` types.
4. Integration tests: rozszerzyć o per-flag wariant lub osobny test pass dla
   `API_DATASOURCE_*=ch`.

Sugerowana kolejność (od najprostszych do najbardziej złożonych):

1. `network` (1 endpoint, `01_get_network_stats.sql`)
2. `ledgers` (2 endpointy, `04`, `05`)
3. `accounts` (2 endpointy, `06`, `07`)
4. `transactions` (2 endpointy, `02`, `03`)
5. `assets` (3 endpointy, `08`-`10`)
6. `nfts` (3 endpointy, `15`-`17`)
7. `contracts` (4 endpointy, `11`-`14`)
8. `liquidity_pools` (5 endpointów — najwięcej, brak reference SQL w 0207 jeszcze)
9. `search` (1 endpoint, complex — trigram + multi-table)

### Step 4: Per-module rollout protocol

Dla każdego modułu:

1. PR z `queries_ch.rs` + handler dispatch + tests
2. Deploy z flag `API_DATASOURCE_<MODULE>=pg` default (no-op deploy, tests CH-path locally)
3. Manual flip `pg → ch` w env config dla staging
4. 24h smoke (CloudWatch monitoring, error rate, latency)
5. Jeśli OK: flip prod env, monitor 7 dni
6. Jeśli problem: rollback flag, debug, retry

### Step 5: Cleanup → 0244

Po wszystkich 9 modułach na `ch` default + 7 dni stable → spawn 0244 (usunięcie
sqlx, queries.rs, DataSource enum simplification).

## Acceptance Criteria (incremental per module)

Dla każdego z 9 modułów:

- [ ] `queries_ch.rs` napisany z CH equivalents
- [ ] Handler dispatch via `DataSource` enum
- [ ] Integration tests pass w `Pg` mode (no regression)
- [ ] Integration tests pass w `Ch` mode (parity verified)
- [ ] Smoke test staging z flag=ch: 24h bez błędów
- [ ] Flip default flag na `ch` w prod env config
- [ ] 7 dni prod monitoring: error rate <0.1%, latency p95 within budget

Cross-cutting:

- [ ] Connection pool initialized at cold-start (no per-request connect)
- [ ] mTLS config working (verified via Caddy access logs `X-Client-Subject: CN=lambda-api-...`)
- [ ] OpenAPI spec niezmieniony (response schema parity) — `nx run @rumblefish/api-types:check-generated` pass
- [ ] **Docs updated** — `docs/architecture/api/api-overview.md` (jeśli istnieje)
      reflects CH-default datastore; per-handler comments wskazują CH queries
- [ ] **API types regenerated** — wymagane jeśli zmieni się schema response (nie powinno);
      uruchom `npx nx run @rumblefish/api-types:generate` przy każdym module PR jako sanity check

## Depends on

- **0241** (CH ma live data od `L_last_closed + 1` — bez tego CH zwraca stale historię)
- **0239 Phase 2** (mTLS connection layer)
- **0240** (RBAC user `api_reader` z odpowiednimi permissions)
- **0207** ✅ (reference CH SQL queries authored — używamy verbatim)

## Open questions

- **Feature flag granularity**: 9 (per-moduł) vs 23 (per-endpoint). Sugerowane
  per-moduł (prostsze); eskalacja per-endpoint jeśli któryś moduł ma problemy.
- **liquidity_pools complex queries**: 5 endpointów ale brak ich w reference set
  z 0207. Może wymagać dodatkowego mapowania (spawn follow-up jeśli okaże się
  duże).
- **search trigram**: w PG używa `pg_trgm` extension. CH ma własne approaches —
  może wymagać re-design lub fallback do PG dla search w pierwszej iteracji.

## Notes

- Feature flag default = `pg` przez całą transition, flip per moduł explicite.
  Operator-driven rollout.
- Po final flip wszystkich 9 modułów → spawn 0244 (cleanup).
- Sąsiad task: 0231 (CH enrichment port) — niezależny, ale obaj korzystają z CH
  connection setup z tego tasku jako precedent.
