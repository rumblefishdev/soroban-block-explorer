# Where the code assumed mainnet (task 0553)

ADR 0052 decides three things: testnet is **configuration, not a code fork**;
its ClickHouse is a **`testnet` database on `ch-prod-01`**, isolated by RBAC
and quota exactly like the `prices` tenant ([[0314]]); **branch = environment**
(`develop → testnet → master`), with "deploy testnet straight from `develop`"
allowed as the first step.

**Already configuration** (checked 2026-09-14): the passphrase flows from
`stellarNetworkPassphrase` into `STELLAR_NETWORK_PASSPHRASE` on all three
Lambdas (`compute-stack.ts:112`) and into Galexie's `network = "testnet"`
(`ingestion-stack.ts:174-175`); indexer and API derive `network_id` from that
env (`crates/indexer/src/handler/process.rs:118`, `crates/api/src/main.rs:201`)
— every remaining `MAINNET_PASSPHRASE` use is a test. Stack names, log groups,
buckets and SSM paths all derive from `envName` (`infra/src/lib/app.ts:34`).
`init.sql` and the queries never qualify `default.` (0 hits) and
`CLICKHOUSE_DATABASE` is plumbed (`crates/db-clickhouse/src/lib.rs:51`), so a
second database needs no schema edit.

**Still says mainnet** — each one is a step below, not a surprise for later:

1. `EnvironmentConfig.envName: 'production'` is a literal type (`types.ts:13`)
   and every `infra/Makefile` target is production-only.
2. `cicd-stack.ts:65` mints one deploy role, bound to GitHub environment
   `production`. The leftover GitHub environment `staging` is not a testnet
   slot — nothing reads it (`docs/deployment.md`, "No staging").
3. The API Lambda gets a hardcoded mainnet `SOROBAN_RPC_URLS`
   (`compute-stack.ts:297-302`); the code defaults are mainnet too
   (`enrichment-shared/src/nft_token_uri/client.rs:43`,
   `api/src/runtime_enrichment/wasm_code.rs:36`). Testnet is
   `https://soroban-testnet.stellar.org`.
4. Runtime enrichment reads ledgers from `aws-public-blockchain` under
   `v1.1/stellar/ledgers/pubnet` (`stellar_archive/mod.rs:31`) — hardcoded.
5. `HetznerDnsStack` creates the `chDomainName` A record from
   `/soroban/<env>/ch-ip`; a second env naming the same host collides. Either a
   `ch-testnet.` alias (Caddy then needs a certificate for it) or reuse the
   production name and skip the stack.
6. The SPA build bakes `cloudflareApiDomainName` + `turnstileSiteKey`
   (`infra/Makefile:109-110`). The Cloudflare-fronted API hostname lives in the
   private `rf-domains` repo, and the Turnstile widget allows hostnames
   explicitly — testnet needs its own of both.
7. The SPA has no notion of network (`VITE_API_BASE_URL`,
   `VITE_TURNSTILE_SITE_KEY` are its only inputs). Without a visible TESTNET
   marker a testnet page is indistinguishable from a mainnet one.

**Found 2026-09-28** (re-check on `develop`; the seven above still hold):

8. Galexie's live task starts at a hardcoded mainnet ledger,
   `START: '63230777'` (`ingestion-stack.ts:263`), and the backfill task
   defaults to `START: '50457424'` (`:356`). Testnet's tip is ~4.9 M, so live
   ingest would wait forever. The comment above `:263` ("TEST VALUE… REVERT
   to '63230777'") is stale — the value already is the revert target.
9. `backfill-runner` keeps its own copies, separate from item 4:
   `partition.rs:11` `ROOT_PREFIX = "v1.1/stellar/ledgers/pubnet"`,
   `snapshot/archive.rs:37` `PUBNET_ARCHIVE` (used with no override by the
   snapshot seed and reconciliation), `snapshot/report.rs:23`
   `LEDGER_FLOOR = 50_457_424` (on testnet every missing row would be filed as
   expected dormancy). Unchanged, a testnet backfill writes MAINNET ledgers into
   the `testnet` database.
10. Nothing detects a reset. Every table is keyed on the ledger sequence
    (`ledgers` `ORDER BY sequence`, entities `ReplacingMergeTree(<ledger>)`):
    after a reset ledger N silently replaces the old N, and entity rows keep
    whichever pre- or post-reset version has the higher ledger. The runbook
    (phase 7) only helps once someone notices.
11. USD prices are safe only while the testnet reader has **no** grant on
    `prices.*`. Without it the price read errors and the API degrades to NULL
    fields (`liquidity_pools/handlers.rs:341`) — correct, but one error log
    line per pool request. With it, native legs map to `("native","XLM","")`
    and testnet XLM pools show mainnet-priced volume and fees: plausible and
    wrong.
    **Decided 2026-09-30: the testnet reader reads `prices.*`.** prices-api has
    no testnet counterpart, so testnet shows mainnet USD prices on purpose;
    native XLM matches, testnet tokens (other issuers and contracts) mostly get
    no price. Shipped in #553.
12. Degrades, not breaks: SEP-1 enrichment resolves almost nothing (issuers'
    `stellar.toml` lists mainnet assets); federation `name*domain` lands on
    mainnet accounts (`web/src/search/federation.ts:135`); the report-issue
    link hardcodes the production host (`web/src/pages/contracts/ContractCode.tsx:59`).

**Reset cadence (2026-09-29):** the Stellar networks docs say resets happen
"2-4 times per year at 17:00 UTC", announced at least two weeks ahead, and name
the next one: **2026-12-16**. Recent resets: 2025-03-19, 2025-08-14 (with
protocol 23), 2025-12-17; none so far in 2026. Testnet RPC on 2026-09-28:
ledger 4,920,864, protocol 28.

**The data lake keeps one folder per genesis**, under
`s3://aws-public-blockchain/v1.1/stellar/ledgers/testnet/`, and the naming is
undocumented. The live chain sits in `2025-12-18/`, one day after its reset;
`2025-12-17/` is an abandoned stub (ledgers 0-14,933), and `2025-06-18/` holds
only a slice. So the prefix changes on every reset, must be configuration, and
the right folder is the one with a `.config.json` that keeps growing — not the
one dated like the reset.
