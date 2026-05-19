### 1. Monorepo package table is incorrect

The table under "One Monorepo, Multiple Languages" does not match the repo today and will not match it after the ClickHouse cutover either. There is no `apps/` directory; Rust code lives in `crates/`, the frontend at `web/`, infra at `infra/`. `libs/domain` does not exist — `domain` is a Rust crate (`crates/domain`). `libs/shared` does not exist. The "eight packages" count understates the workspace by roughly 2×.

**Suggested action:** Rewrite the table against the post-CH target. Target-state crates (per author confirmation): `indexer`, `enrichment-worker`, `api`, `db` (renamed from `db-clickhouse`), `xdr-parser`, `domain`, `backfill-runner`, `backfill-enrichment-runner`, `enrichment-shared`, `db-merge`. Plus `web/`, `infra/`, `libs/api-types`, `libs/ui`. Crates planned for removal (`db` legacy PG, `db-migrate`, `db-partition-mgmt`, `backfill-bench`, `audit-harness`) should not appear.

### 2. Event Interpreter Lambda section describes a component that does not exist

The "Lambda (Rust) for the Event Interpreter" section, including the "Soroswap / Aquarius / Phoenix → 'Swapped 100 USDC for 95.2 XLM'" example and the EventBridge 5-minute trigger, describes a design that was explicitly removed in [ADR 0007](https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/lore/2-adrs/0007_simplified-2-lambda-architecture.md). The actual third Lambda (`enrichment-worker`) is **SQS-driven** (see [`compute-stack.ts:279`](https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/infra/src/lib/stacks/compute-stack.ts#L279)) and performs **SEP-1 `stellar.toml` fetch for assets plus NFT token-URI resolution** (see [`crates/enrichment-shared/src/lib.rs`](https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/crates/enrichment-shared/src/lib.rs)). It does not pattern-match DeFi protocols.

**Action:** Rewrite the section to reflect the SQS-driven SEP-1 / NFT URI enrichment, or remove it.

### 3. "No external API" claim is absolute but the system does call external APIs

The article header states no external API is called. In practice:

- `enrichment-shared` calls `stellar.toml` (SEP-1) for asset metadata
- `backfill-runner` `bootstrap_account_state` calls Soroban RPC `getLedgerEntries` once per backfill window to hydrate account skeletons (accounts referenced as `transaction_participants` but never mutated in-window, so they sit as `sequence_number=0, home_domain=null`)
- `audit-harness/horizon-diff` calls Horizon (tooling, not pipeline)

The live ingestion + serving path (Galexie → S3 → Ledger Processor → DB → API) is genuinely free of external chain dependencies. The article should reflect that scope.

**Suggested action:**

- Rename the section header to "Why our live pipeline doesn't call any external chain API"
- Add one sentence in the backfill section: _"For the historical backfill, we use Soroban RPC `getLedgerEntries` as a one-time bootstrap to hydrate account states that were referenced in the window but never mutated inside it — a fixup that runs once per window and never in the live path."_

---

## Wording / Accuracy Nits

### 4. "aws-sdk-rust" crate name

The AWS Rust SDK ships as per-service crates (`aws-sdk-s3`, `aws-sdk-dynamodb`, etc.), not a single `aws-sdk-rust` package. Minor, but easy to fix.
