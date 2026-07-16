---
name: compare-with-stellar-api
description: Verify ClickHouse query output against Horizon, stellar.expert, and independently decoded raw XDR.
---

# /compare-with-stellar-api — Verify a ClickHouse query against Stellar APIs + raw XDR

Take a path to a ClickHouse SQL query, execute it against the **local Docker
ClickHouse**, select representative rows, and verify the same rows in parallel
against Horizon, stellar.expert, and independently decoded raw XDR.

The result is an evidence report, not an implementation task. Aggregate the
findings and **STOP**; do not edit code, commit, or create a lore task.

## Scope and safety

- This is **ClickHouse-only**. Use only
  `docs/architecture/database-schema/endpoint-queries-clickhouse/` and the
  ClickHouse client workflow defined below.
- Default target is the local `clickhouse` Docker Compose service. Never query
  production ClickHouse unless the user explicitly supplies and authorizes a
  production connection.
- Canonical query files live in
  `docs/architecture/database-schema/endpoint-queries-clickhouse/` and the
  project runner is `run_endpoint_ch.sh` in that directory.
- A reference SQL file can lag the live Rust query. Before asserting that an
  endpoint implementation is correct, compare its shape to the corresponding
  `crates/api/src/**/queries_ch.rs` path. In particular, transaction list
  aggregation in the live code is a non-correlated two-step query; do not
  validate a stale correlated-subquery reference as if it were production.

## Argument

`/compare-with-stellar-api <path-to-clickhouse-sql-file>` is required. If the
argument is empty, an endpoint name, a table name, or a SQL path outside the
ClickHouse endpoint-query directory,
**STOP** and ask for a path.

## Step 1 — Read the SQL and select a statement

1. Read the file and verify it is ClickHouse SQL.
2. Count `-- @@ split @@` separators.
   - No separator: `selected_statement = 1`.
   - One or more: enumerate each statement as A, B, C, … using its leading
     comment or SQL keyword, then **STOP** and ask the user which statement to
     verify. Do not select one automatically.
3. Carry `selected_statement` into every later step.
4. For canonical endpoint files, read the matching endpoint section in
   `endpoint-queries-clickhouse/README.md` and inspect the equivalent live
   Rust CH query before sampling. Report `reference query matches live path`,
   `reference-only`, or `diverges`; a divergent reference may still be
   syntax-checked, but cannot certify the live implementation.

## Step 2 — Execute against local Docker ClickHouse

Boot and verify the local canonical schema first:

```bash
docker compose up -d clickhouse db-clickhouse-init
docker compose exec -T clickhouse clickhouse-client \
  --user=default --password=clickhouse --database=default \
  --query='SELECT 1'
```

Set these variables once for all commands below; callers may override the
defaults when their local Compose setup differs:

```bash
export SBE_CH_SERVICE=clickhouse
export SBE_CH_USER=default
export SBE_CH_PASS=clickhouse
export SBE_CH_DB=default
```

Every selected statement must ultimately produce **one JSON object per row**:
`FORMAT JSONEachRow`. Do not use TSV output as the sampling input.

### Mode A — canonical endpoint-query file

Path matches
`docs/architecture/database-schema/endpoint-queries-clickhouse/<NN>_*.sql`.

1. Run the supplied runner to discover real values and validate the endpoint
   orchestration:

   ```bash
   docs/architecture/database-schema/endpoint-queries-clickhouse/run_endpoint_ch.sh <NN>
   ```

2. Reuse the concrete values printed by the runner, extract only
   `selected_statement`, substitute its `$1`, `$2`, … placeholders exactly as
   the runner did, remove its trailing semicolon, and append
   `FORMAT JSONEachRow`.
3. Run the resulting query through the local service:

   ```bash
   docker compose exec -T "$SBE_CH_SERVICE" clickhouse-client \
     --user="$SBE_CH_USER" --password="$SBE_CH_PASS" --database="$SBE_CH_DB" \
     --query='<selected statement with concrete literals> FORMAT JSONEachRow'
   ```

For statements whose inputs depend on an upstream statement, preserve the
upstream value chosen by `run_endpoint_ch.sh`; do not invent a second sample.

### Mode B — ad-hoc ClickHouse query

1. Extract `selected_statement` by splitting on `-- @@ split @@`.
2. Parse the `Inputs:` header for `$N` placeholders. Use its ClickHouse type
   and semantics to write small local discovery queries. Use literal `NULL`
   only where the query expects a nullable parameter; use CH-native literals
   such as `toInt64(123)`, `unhex('…')`, or quoted strings for concrete values.
3. Substitute placeholders from highest number to lowest so `$10` cannot be
   partially replaced as `$1`.
4. Remove the final semicolon and append `FORMAT JSONEachRow`, then execute it
   with `clickhouse-client` as in Mode A.

### Stop conditions

- Query error: report the exact ClickHouse error and **STOP**.
- Zero JSON rows: report `empty result set; populate local ClickHouse or check
the query` and **STOP**.
- One row: use it as the only sample and skip variance selection.
- Otherwise aim for at least 20 rows before choosing five samples.

## Step 3 — Select five representative rows

Parse the JSONEachRow output. Do not take the first five rows.

- Include both null and non-null values for nullable columns when available.
- Cover enum/type fields such as `asset_type`, `event_type`, and `op_type`.
- Cover both values of booleans such as `successful`, `has_soroban`, and
  `is_sac` when present.
- Prefer non-trivial arrays and aggregates.
- Reserve one or two genuinely random rows.

For each selected row, write a one-line explanation of what it demonstrates.

## Step 4 — Build the verification contract

Determine one entity type: `transaction`, `account`, `contract`, `asset`,
`ledger`, `nft`, or `liquidity_pool`.

For canonical files, use the output fields of `selected_statement` plus the
endpoint response-shape documentation. Include only fields physically supplied
by ClickHouse. Exclude API-only fields from Soroban RPC, S3, archive/XDR
overlays, cursors, positions, and other synthesized values. For ad-hoc files,
use projected column names only.

Record any reference-vs-live-Rust divergence found in Step 1 before dispatch.

## Step 5 — Dispatch three parallel verifiers

Use the host runtime's parallel-agent mechanism to dispatch all three at once.
Each receives the **identical** selected rows, descriptions, and field list.
Do not let a verifier choose its own rows.

Use this prompt structure verbatim, filling placeholders:

```
You are verifying Stellar entity data returned by a local ClickHouse query
against {SOURCE_NAME} ({SOURCE_BASE_URL}).

Entity type: {ENTITY_TYPE}
URL pattern hint: {URL_PATTERN_HINT}

For every supplied row, fetch the corresponding entity and compare every field
in the list. Do not guess.

Rows to verify:
{ROWS_WITH_DESCRIPTIONS}

Fields to verify:
{FIELD_LIST}

Output exactly:
Row 1 (<description verbatim>):
  - <field>: MATCH
  - <field>: MISMATCH (CH=<value>, source=<value>)
  - <field>: SOURCE_MISSING (<reason>)
  - <field>: NOT_APPLICABLE (<reason>)
  - <field>: UNVERIFIABLE (<reason>)
Row 2 (<description>): …

Notes: <rate limits, partial fetches, source limitations>

Hard rules:
- `SOURCE_MISSING` means the source lacks the entity; it is not a mismatch.
- `MISMATCH` requires a present source value that differs.
- If extraction fails, use `UNVERIFIABLE`, never an inferred value.
- Use exactly the supplied rows and field list.
```

Dispatch with these source-specific instructions:

| Source         | Base / URL pattern                                                                                                                                             | Scope                                                                                                                                      |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Horizon API    | `https://horizon.stellar.org`; `/transactions/<hash>`, `/accounts/<id>`, `/ledgers/<seq>`, `/assets?asset_code=<c>&asset_issuer=<i>`, `/liquidity_pools/<hex>` | Horizon does not cover Soroban contracts, events, invocations, or NFTs; mark those fields `NOT_APPLICABLE`.                                |
| stellar.expert | `https://stellar.expert/explorer/public`; `/tx/<hash>`, `/account/<id>`, `/asset/<code>-<issuer>`, `/contract/<id>`, `/liquidity-pool/<hex>`                   | Mark unavailable or JS-only fields `UNVERIFIABLE`.                                                                                         |
| Raw XDR        | Fetch Horizon transaction XDR with `curl`, then decode independently                                                                                           | Applies to transactions and per-transaction facts only. Accounts, aggregate assets, ledgers, NFTs, and pool reserves are `NOT_APPLICABLE`. |

### Raw XDR verifier requirements

1. Fetch `envelope_xdr`, `result_xdr`, and `result_meta_xdr` with `curl` and
   `jq`; do not use a browser fetcher for base64 blobs.
2. Prefer the external-SSD venv installed for this workspace:

   ```bash
   target/stellar-sdk-venv/bin/python3 -c 'import stellar_sdk'
   ```

   If absent, try `STELLAR_SDK_PYTHON`, then `python3`, then a user-local venv.
   `stellar-cli` is a classic-envelope fallback only; skip it for Soroban
   envelopes. Do not compile a Rust decoder just for this verification.

3. Compare facts from decoded XDR, for example transaction success, operation
   count, source account, fee charged, memo, operation bodies, event topics and
   data, and invocation arguments/return values.
4. If no suitable decoder is available, mark the relevant fields
   `UNVERIFIABLE` and explain why.

Raw XDR has highest authority. If it agrees with ClickHouse and explorers
disagree, treat the explorer as the likely faulty display layer.

## Step 6 — Frontend contract check

Search `docs/architecture/frontend/frontend-overview.md` for the endpoint or
route. If found, compare its required fields with the selected ClickHouse query
projection and report missing required fields or unconsumed projections. If
absent, report: `Frontend contract check skipped — no matching route section`.

## Step 7 — Report and stop

Present:

1. the reference-vs-live-Rust status;
2. sampled rows and why each was selected;
3. a compact source matrix per row;
4. pure mismatches, prioritizing those confirmed by Raw XDR;
5. all-sources-missing rows;
6. frontend-contract result; and
7. caveats such as partial local CH data, rate limits, or unavailable XDR.

Then **STOP** and wait for the user's next instruction.

## Anti-patterns

- Querying a non-ClickHouse database or treating its result as ClickHouse evidence.
- Querying production ClickHouse without explicit authorization.
- Using `FORMAT TabSeparated` instead of JSONEachRow for sample selection.
- Treating the reference SQL as the live Rust implementation without checking.
- Allowing verifier agents to pick their own rows.
- Passing non-ClickHouse/API-synthesized fields to verifiers.
- Confusing `SOURCE_MISSING` with `MISMATCH`.
- Creating code changes, commits, or lore tasks from a validation report.
