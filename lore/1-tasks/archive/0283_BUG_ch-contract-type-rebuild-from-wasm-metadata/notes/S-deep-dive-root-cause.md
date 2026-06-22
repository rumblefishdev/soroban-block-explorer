# S — Deep-dive: why prod CH hot NFT tables are empty (root cause + full context)

> Synthesis note, 2026-06-10, karolkow (deep-dive session with Claude).
> Status: mature. Spawned the parent task 0283.

## Trigger

While testing 0231 (NFT `token_uri` enrichment) pre-prod, discovered prod
CH state: hot `nfts` = 0, `nft_ownership` = 0, `nfts_pending` = 59,728,965,
`nft_ownership_pending` = 138,465,599 (live numbers 2026-06-10, measured by
stkrolikiewicz). NFT endpoints E15/E16/E17 serve nothing. Initial theory
(from team discussion): "reclassification not yet scheduled, gated on WASM
classification completeness". Deep-dive disproved that — see Root cause.

## Root cause chain (all verified in code, file:line)

1. **Per-ledger stage granularity.** `stage::prepare` /
   `prepare_with_sac_overrides` covers exactly one ledger
   (`crates/db-clickhouse/src/persist.rs:63-99`).

2. **WASM classification map is same-ledger only.**
   `crates/db-clickhouse/src/persist/stage.rs:343-363` builds
   `wasm_classification: HashMap<[u8;32], ContractType>` from THIS call's
   `contract_interfaces` (i.e. WASM uploads parsed in THIS ledger).

3. **Deploy-time verdict override requires same-ledger coincidence.**
   `stage.rs:376-389`, comment verbatim: _"if this deployment's wasm was
   classified in the same ledger"_. On Soroban, `uploadContractWasm` and
   `createContract` are separate transactions (InvokeHostFunction = 1
   op/tx) — almost always different ledgers. So non-SAC deploys persist
   parser default `Other` (=1).

4. **"Re-emission on next observation" (ADR 0046) is not implemented.**
   `route_for` (`stage.rs:909-918`) reads `verdict_by_contract` built only
   from THIS ledger's `out.contract_rows`; contract sources are: in-window
   deploys, name-writes (`contract_type: None` → skipped), SAC overrides,
   pass-2 stubs (`None` → skipped). A later NFT event from an
   already-classified contract finds no map entry → routes to Pending.
   **Hot tables can never fill organically on CH.**

5. **No post-hoc verdict rebuild exists.**

   - `repair_tier1.rs:327` — `sc.contract_type` passthrough (rebuilds only
     deployer_id/deployed_at_ledger).
   - `nft_reclassify.rs:193-194` — promote keyed on
     `soroban_contracts.contract_type = 2`, which nothing populates.

6. **Empirical confirmation.** 0228 Phase 5 `nft-reclassify` on the full
   merged backfill (2026-05-21): `promoted_nfts=0`,
   `dropped_pending_nfts=27,602,309` (types 0/3 = SAC/fungible),
   `dropped_pending_ownership=60,492,304`
   (`docs/runbooks/artifacts/phase6_validation_20260521.md`).

7. **Live ingest shares the gap.** Post-0241 cutover (2026-05-29) the
   indexer persists via the same path
   (`crates/indexer/src/handler/mod.rs:30` →
   `persist_ledger_clickhouse`). Pending grew 48.8M→59.7M / 112.3M→138.5M
   between 2026-05-21 and 2026-06-10 (~1M+/day combined), incl. fresh SAC
   leak (8.6M in nfts_pending, 18.9M in ownership_pending per
   stkrolikiewicz live measurement) because 0221's write-time leak is
   also unfixed.

## The missing piece is small

Classification INPUT is already in CH: `wasm_interface_metadata`
(3,216 distinct WASM hashes; `metadata` JSON = `{"functions":
[{name, doc, inputs, outputs}], "wasm_byte_len"}` — written at
`stage.rs:354-362`). The classifier is a pure function over function
names (`crates/xdr-parser/src/classification.rs:101-120`):

- any of `owner_of | token_uri | approve_for_all | get_approved |
is_approved_for_all` → **Nft** (=2)
- else any of `decimals | allowance | total_supply` → **Fungible** (=3)
- else **Other** (=1)

Missing step = JOIN `soroban_contracts.wasm_hash` ×
`wasm_interface_metadata` → classify → rebuild `contract_type`. After
that, the EXISTING `backfill-runner nft-reclassify` does the whole
promote/drop for BOTH pending tables.

## Corrections to the prior team understanding (Slack 2026-06-10)

| Prior claim                                                     | Actual                                                                                                                                                                                                                                                                             |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "drain for `nft_ownership_pending` not written"                 | Written — `nft_reclassify.rs:99-128` handles both tables (promote + drop + legacy cleanup + OPTIMIZE), idempotent. Manual runbook 0221 is a subset of it.                                                                                                                          |
| "reclassification gated on WASM classification completeness"    | WASM universe is complete in-range (backfill starts at Soroban genesis 50,457,424, so every mainnet `uploadContractWasm` is in-range). Gated on the missing rebuild step — tooling gap, not data gap.                                                                              |
| "~170M Other rows await WASM promote/drop"                      | After rebuild, the overwhelming majority drops: 309,385 of 321,364 contracts are SAC/fungible (96%); true-Nft population likely tiny (one verified mainnet NFT known: Bachini `CDA5FGE4…`, SEP-39, i128 token_id — see ADR 0046 Alt 4). 198M pending rows ≠ 198M of decision work. |
| "run 0221 drain, then write ownership mirror, then 0217 Part 2" | Correct sequence: `contract-type-rebuild` (new) → `nft-reclassify` (exists). TRUNCATE (0217 Part 2) is a separate, later decision.                                                                                                                                                 |

## Prod state snapshot (for before/after comparison)

| Metric                        | 2026-05-21 (Phase 6)   | 2026-06-10 (live)    |
| ----------------------------- | ---------------------- | -------------------- |
| `nfts` (hot)                  | 0                      | 0                    |
| `nft_ownership` (hot)         | 0                      | 0                    |
| `nfts_pending`                | 48,854,535             | 59,728,965           |
| `nft_ownership_pending`       | 112,301,444            | 138,465,599          |
| SAC-leak in nfts_pending      | 0 (just dropped 27.6M) | 8,628,683 (14.4%)    |
| SAC-leak in ownership_pending | 0 (just dropped 60.5M) | 18,931,625 (13.7%)   |
| `soroban_contracts` total     | 321,364                | —                    |
| …of which SAC/fungible        | —                      | 309,385              |
| `wasm_interface_metadata`     | 3,216                  | —                    |
| Nft-classified (type=2)       | 0 (`promoted_nfts=0`)  | — (expect 0; verify) |

Backfill: complete + validated, ledgers 50,457,424–62,527,999, gaps=0,
Horizon parity 980/980 (task 0228, closed 2026-05-22). Live tail since
2026-05-29 (task 0241).

## Verification queries (Step 0 of parent task — run on prod CH)

```sql
-- Q1. Verdict breakdown (expect contract_type=2 → 0)
SELECT contract_type, count()
  FROM soroban_contracts FINAL
 GROUP BY contract_type ORDER BY contract_type;

-- Q2. Would-be-Nft contracts after rebuild (THE sizing number)
SELECT count()
  FROM soroban_contracts sc FINAL
 INNER JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash
 WHERE sc.is_sac = false
   AND arrayExists(
         n -> n IN ('owner_of','token_uri','approve_for_all',
                    'get_approved','is_approved_for_all'),
         JSONExtract(wim.metadata, 'functions',
                     'Array(Tuple(name String))').1
       );

-- Q3. Bachini SorobanNFT sanity (known real mainnet NFT)
SELECT id, contract_type, is_sac, wasm_hash IS NOT NULL AS has_wasm
  FROM soroban_contracts FINAL
 WHERE contract_id = 'CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY';

-- Q4. Pending volume under would-be-Nft contracts (= promote volume)
SELECT
  (SELECT count() FROM nfts_pending WHERE contract_id IN (
     SELECT sc.id FROM soroban_contracts sc FINAL
      INNER JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash
      WHERE sc.is_sac = false AND arrayExists(
        n -> n IN ('owner_of','token_uri','approve_for_all',
                   'get_approved','is_approved_for_all'),
        JSONExtract(wim.metadata, 'functions',
                    'Array(Tuple(name String))').1))) AS nfts_promote,
  (SELECT count() FROM nft_ownership_pending WHERE contract_id IN (
     SELECT sc.id FROM soroban_contracts sc FINAL
      INNER JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash
      WHERE sc.is_sac = false AND arrayExists(
        n -> n IN ('owner_of','token_uri','approve_for_all',
                   'get_approved','is_approved_for_all'),
        JSONExtract(wim.metadata, 'functions',
                    'Array(Tuple(name String))').1))) AS ownership_promote;
```

Notes: `JSONExtract` tuple-shape against `{"functions":[{"name":...}]}`
needs a quick sanity check on one row first (`SELECT
JSONExtractArrayRaw(metadata, 'functions') FROM wasm_interface_metadata
LIMIT 1`); if the Tuple cast fights, fall back to
`arrayExists(x -> JSONExtractString(x, 'name') IN (...),
JSONExtractArrayRaw(metadata, 'functions'))`. The Rust implementation in
the parent task does NOT depend on this SQL — it reuses
`classify_contract_from_wasm_spec` directly.

Access: mTLS cert via `infra-hetzner/ca/issue-client-cert.sh` +
`~/.config/soroban-prod.env` from password manager
(`soroban-prod / ansible-env`); see `infra-hetzner/README.md` and
`docs/architecture/security/clickhouse-rbac.md`.

## Downstream dependency map

```
0283 (this) ──┬─► 0259  E15/16/17 validation (hot tables finally non-empty)
              ├─► 0231  Step 4 prod enrichment drain (queue non-empty)
              ├─► 0282  NFT media-url quality (needs >1 real NFT sample)
              ├─► 0221  subsumed at drain level (write-time leak still open
              │         → live-gap follow-up)
              └─► 0217  Part 2 TRUNCATE (separate later decision)
```
