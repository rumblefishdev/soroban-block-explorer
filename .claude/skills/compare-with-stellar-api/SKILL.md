---
name: compare-with-stellar-api
description: Verify ClickHouse query output against Horizon, stellar.expert, and independently decoded raw XDR.
---

# /compare-with-stellar-api — Verify a ClickHouse query against Stellar APIs + raw XDR

Take an API endpoint's ClickHouse query (or an ad-hoc ClickHouse SQL file),
execute it against the **local Docker ClickHouse**, select representative rows,
and verify the same rows in parallel against Horizon, stellar.expert, and
independently decoded raw XDR.

The result is an evidence report, not an implementation task. Aggregate the
findings and **STOP**; do not edit code, commit, or create a lore task.

## Scope and safety

- This is **ClickHouse-only**. The endpoint SQL reference is the Rust query
  function that runs it, in `crates/api/src/<module>/queries…`
  ([ADR 0060](../../../lore/2-adrs/0060_rust-queries-are-the-endpoint-sql-reference.md)).
  There is no separate SQL copy to read.
- Queries execute against the local `clickhouse` Docker Compose service. Never
  run a query against production ClickHouse unless the user explicitly
  authorizes it.
- The one production read this skill may propose is `chq` on
  `system.query_log`, to see the exact SQL text the API executed (Step 1). It
  reads query metadata, not table data; still ask before running it, and skip
  it when the Rust query is enough.

## Argument

`/compare-with-stellar-api <target>` is required. `<target>` is one of:

- an endpoint, e.g. `GET /v1/assets/:id/transactions`;
- a Rust query location, e.g. `crates/api/src/assets/queries.rs::fetch_transactions`;
- a path to an ad-hoc ClickHouse `.sql` file (Mode B).

If the argument is empty, a bare table name, or does not resolve to one of
these, **STOP** and ask.

## Step 1 — Find the query and select a statement

1. **Endpoint or Rust location (Mode A).** Find the handler in
   `crates/api/src/<module>/handlers.rs` (the `#[utoipa::path]` names the
   route) and follow it to the query function it calls in
   `crates/api/src/<module>/queries.rs` or `queries/<name>.rs`.
2. Enumerate the statements the path issues: every `client.query(…)` call
   reached for this request is one statement. Label them A, B, C, … by what
   they read (driver seek, page fetch, aggregate, StrKey resolve).
   - One statement: `selected_statement = A`.
   - More than one: list them, then **STOP** and ask the user which to verify.
     Do not select one automatically.
3. Read the comments beside the selected statement: they state the index
   choice, the dedup rule (`FINAL` / `LIMIT 1 BY` / aggregate) and known
   traps. Carry them into the report.
4. Build the executable SQL:

   - Copy the SQL string. Replace each `?` with its `.bind(…)` value in order,
     and each `format!` placeholder (`{order}`, `{limit}`, key lists,
     keysets) with the value the code would build for the chosen inputs.
   - **Optional, exact text:** with the user's go, read the SQL the API
     actually executed from production:

     ```bash
     chq "SELECT event_time, read_rows, query FROM system.query_log
          WHERE event_date = today() AND type = 'QueryFinish'
            AND user = 'api_reader' AND query LIKE '%<distinctive fragment>%'
          ORDER BY event_time DESC LIMIT 3 FORMAT Vertical"
     ```

     Pick a fragment unique to the statement (a table plus a predicate).
     `chq` exits 0 even on a server error — check the output for
     `DB::Exception`. The text carries production literals (surrogate ids,
     head sequence); replace them with values discovered locally in Step 2.

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

### Mode A — an endpoint's Rust query

1. Discover real input values from local ClickHouse with small queries that
   follow the query's own lookups (e.g. a StrKey → surrogate id via the same
   table the Rust resolves it from, a head sequence via `max(sequence)` from
   `ledgers`).
2. For a statement that consumes an earlier one's output (a page fetch keyed
   by a driver's positions), run the earlier statement first and reuse its
   values; do not invent a second sample.
3. Append `FORMAT JSONEachRow` to the substituted statement and run it:

   ```bash
   docker compose exec -T "$SBE_CH_SERVICE" clickhouse-client \
     --user="$SBE_CH_USER" --password="$SBE_CH_PASS" --database="$SBE_CH_DB" \
     --query='<selected statement with concrete literals> FORMAT JSONEachRow'
   ```

The module's ClickHouse tests (`decode_smoke.rs` / `ch_tests.rs`, gated on
`CH_URL`) show how the code itself bootstraps inputs; reuse their discovery
queries where they fit.

### Mode B — ad-hoc ClickHouse query

1. Extract `selected_statement` by splitting on `-- @@ split @@`; with more than
   one statement, list them and **STOP** for a choice, as in Step 1.
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

For Mode A, use the output fields of `selected_statement` plus the endpoint's
response DTO (`crates/api/src/<module>/dto.rs`). Include only fields physically
supplied by ClickHouse. Exclude API-only fields from Soroban RPC, S3,
archive/XDR overlays, cursors, positions, and other synthesized values. For
Mode B, use projected column names only.

Record any difference found in Step 1 between the Rust query and the executed
text from `system.query_log` before dispatch.

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

1. the query verified (Rust function + statement label, or file path) and,
   when read, whether the `system.query_log` text matched it;
2. sampled rows and why each was selected;
3. a compact source matrix per row;
4. pure mismatches, prioritizing those confirmed by Raw XDR;
5. all-sources-missing rows;
6. frontend-contract result; and
7. caveats such as partial local CH data, rate limits, or unavailable XDR.

Then **STOP** and wait for the user's next instruction.

## Anti-patterns

- Querying a non-ClickHouse database or treating its result as ClickHouse evidence.
- Querying production ClickHouse without explicit authorization (the
  `system.query_log` read in Step 1 included).
- Using `FORMAT TabSeparated` instead of JSONEachRow for sample selection.
- Verifying a hand-copied query instead of the Rust function the API runs.
- Allowing verifier agents to pick their own rows.
- Passing non-ClickHouse/API-synthesized fields to verifiers.
- Confusing `SOURCE_MISSING` with `MISMATCH`.
- Creating code changes, commits, or lore tasks from a validation report.
