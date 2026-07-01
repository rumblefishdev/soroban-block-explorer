---
id: '0331'
title: 'FEATURE: bespoke Soroban-token total_supply + holder_count via event-fold MV'
type: FEATURE
status: active
related_adr: ['0043', '0044']
related_tasks: ['0304', '0194', '0210', '0138', '0243', '0323']
tags: [clickhouse, soroban, assets, enrichment, effort-medium, milestone-2]
milestone: 2
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/assets/queries_ch.rs
  - crates/xdr-parser/src/scval.rs
history:
  - date: '2026-06-26'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0304 metadata-validation session. Bespoke Soroban fungible
      tokens (asset_type=3) render "—" for Total supply + Holders in the assets
      list. Prototype validated 2026-06-26 that an event-fold over soroban_events
      reproduces on-chain total_supply() exactly (6/6 sample). New angle vs the
      scoped-out 0138 (storage-scan); covers the type-3 gap that 0194 (types 1,2)
      and 0210 (classic parity) leave open.
  - date: '2026-06-26'
    status: active
    who: karolkow
    note: Promoted to active to start implementation.
---

# FEATURE: bespoke Soroban-token total_supply + holder_count via event-fold MV

## Summary

Surface `total_supply` and `holder_count` for **bespoke Soroban fungible tokens**
(`asset_type = 3`) in the assets list/detail — today `—` for every Soroban token.
The trigger is type-3, but the chosen fix is the **fundamental balance model**
(Option C): a single unified per-holder `balances` table from ledger STATE, raw+decimals
representation for all asset types. (Original event-fold plan REFUTED — see CURRENT PLAN.)

> ⚠️ The H1/filename still say "event-fold MV" (historical). The actual approach is
> ledger-state + Option C — see **CURRENT PLAN** below; everything under "DECISION",
> "Implementation", "Findings", and "Implementation Plan (SUPERSEDED)" is trail.

## Path X 2026-07-01 (karolkow) — contract-held 0/1 LIVE via symmetric keying (SUPERSEDES key-by-type-1)

Authoritative for the contract-held classic/native leg (old D2). **Supersedes** the CURRENT PLAN
line "keyed by classic/native `asset_id` (type-1), 0339-forward-compatible": we now key contract-held
balances by the **storing contract's own surrogate**, exactly like type-3 — no map, no new table.

### The two problems
- **Problem A — decode the value.** A contract holds a classic/native asset as a `Balance(Address)`
  `ContractData` entry inside that asset's **SAC**, valued as the `BalanceValue { amount, authorized,
  clawback }` **struct** (not the bare `i128` a type-3 token uses). The type-3 path dropped it.
- **Problem B — assign the `asset_id`.** The SAC `contract_id` is a one-way hash of the asset, so a
  balance change alone can't yield `(code, issuer)`.

### Decision — Path X (symmetric, no map)
`ids::asset_id(_, contract)` returns the **contract surrogate for BOTH type-2 (SAC) and type-3**
(verified `ids.rs:132`; golden test `asset_id(2,…,csac)==csac`, `asset_id(3,…,ctok)==ctok`). So a SAC
balance emitted with `contract_id = SAC` lands on that SAC's **existing type-2 asset row** — the same
symmetric rule type-3 uses for its own row. **Task 0339 folds type-2 → type-0/1** (it already folds
the trustline aggregates; `balances` rides the same fold). This dissolves Problem B entirely: no
registry, no prefetch, no routing-by-shape, no silent-fallback — we never re-identify the asset at
write time, we key by the storing contract, which is always correct.

**Rejected alternatives** (all needed the SAC→asset map, which Path X avoids): (1) durable map on
`soroban_contracts` + write-side prefetch; (2) read-side JOIN fold (pushes cost to every reader,
gotcha #19b ×100 risk); (3) forward-compute reverse map (indexer is Lambda/stateless → ~322k-asset
recompute per batch). See the devil's-advocate pass in the session trail.

### Done (branch, TDD, all green — 284 xdr-parser + 65 db-clickhouse + 20 indexer, clippy clean)
- `decode_sac_balance_value` + `SacBalanceValue { amount, authorized, clawback }` (`state.rs`), strict:
  rejects any non-`{amount,authorized,clawback}` map (`_ => return None`).
- Wired into `extract_soroban_token_balances`: bare `i128` → type-3; SAC struct → `.amount`. **~5 lines;
  staging UNCHANGED** (already keys by contract surrogate). Live path parser→staging→balances→aggregate
  works for contract-held 0/1 on the branch.
- Real-mainnet NON-circular tests: `decode_sac_balance_value_real_mainnet`,
  `extract_sac_struct_balance_real_mainnet`, `decode_sac_balance_value_rejects_foreign_maps`.

### Validation (pool `CATUJXDU…`, native XLM + classic EURC, 2026-07-01) — 4 sources, exact match
| source | XLM (raw) | EURC (raw) | independence |
|---|---|---|---|
| our SAC decode (parser) | 11 668 057 013 216 | 2 020 807 612 134 | — |
| SAC `balance()` getter | same | same | semi (same entry) |
| pool `get_reserves()` | same | same | **full** (protocol's own storage) |
| StellarExpert (web) | 1 166 805.7013216 | 202 080.7612134 | **full** (3rd party) |

Rigorous for decoder correctness; NOT yet for edge classes (vault, rebasing, TTL-archived, many
holders) — that's the OPS `O5` ≥10-token pass.

### Coordination + open items
- **[0339 MUST]** When 0339 folds type-2 → type-1, it MUST re-key `balances.asset_id` (type-2 →
  type-0/1) too, else contract-held silently vanishes when the type-2 asset rows are deleted. Add to
  0339 acceptance criteria.
- **authorized/clawback**: decoded into `SacBalanceValue` but NOT propagated (only `amount` used).
  Frozen-balance policy (count vs exclude; UI "frozen"/"clawback" badge) is open — data is captured, so
  a later decision needs no re-backfill.
- native by ACCOUNT is unchanged (AccountEntry → `asset_id(0)` direct); only contract-held native goes
  via the native SAC's type-2 row → 0339 fold.

### Historical backfill for contract-held 0/1 — LIGHT seed, NOT S3 reprocess (2026-07-01) — BUILT
An S3 `Run` works (shared parser → same decode) but is heavy (12.8M ledgers × decompress+XDR-parse).
Instead the existing `balance-seed` was **extended** (DONE — reuses candidate-scan → RPC →
`build_balance_rows`):
- `read_sac_seed_candidates` — C-addresses scraped from SAC events (full canonical vocabulary
  `transfer/mint/burn/clawback/set_authorized`), scoped by `assets asset_type=2`, deduped with
  `groupUniqArrayArray` (memory-bounded — a plain `groupArrayArray` would OOM on high-traffic SACs).
- `decode_balance_entry` (rpc_snapshot) decodes the SAC struct by **reusing** the live parser's
  `xdr_parser::decode_sac_balance_value` (via `scval_to_typed_json`) — ONE decoder, lock-step by
  construction. Keyed by the SAC surrogate (Path X). Both value shapes flow through one pipeline.
- Real-mainnet non-circular tests (`decode_balance_entry_sac_struct_real_mainnet`) + gated CH
  integration test (`read_sac_seed_candidates_scrapes_contract_holders_only`).

**OPS to run it (not done):**
- **`--dry-run` FIRST** to benchmark the ~4.46B-row scan + read the funnel counts before a live write.
- **REQUIRED validation gate**: cross-check the seed's per-SAC sum against pools independently readable
  via `get_reserves()`; a missed known pool = incomplete enumeration. Log dropped counts; never claim 100%.
- Completeness is unverifiable in general (event-scrape can't prove it found every holder; most SACs have
  no "all holders" oracle) — accept + document the residual. `authorized=false` (frozen) balances are
  seeded into supply and their magnitude is currently unmeasured (flags decoded but not threaded) —
  decide count-vs-exclude before relying on the number.

### OPS sequencing (ordered) — NO step needs a halted indexer; only ONE thing precedes the deploy
The CH indexer is **single-write** (`writer.rs:107`: legacy `account_balances_current` insert removed;
classic+native stage straight into `balances`). So deploying the indexer is a **cutover**, not a
dual-write. Deploy the indexer FIRST so it feeds `balances` live, then fill history behind it — no gap,
no re-runs.

1. **[DB, BEFORE indexer deploy] `ALTER TABLE assets ADD COLUMN id`** — the ONLY pre-deploy step.
   `CREATE IF NOT EXISTS` can't add it; without it the indexer's `assets` insert fails / rows get `id=0`
   → every read joins on `assets.id` → empty supply+portfolios. **Hard gate; verify `count(id=0)=0`
   after backfill.** (Blocker C1.)
2. **[deploy indexer]** — init.sql creates `balances`+`balance_aggregates`; live writes begin; `abc` frozen.
3. **[DB] migrations (AFTER deploy)** — `assets.id` backfill (`WHERE id=0`); classic/native
   `account_balances_current` → `balances` (captures the now-frozen `abc`).
4. **[run] catch-up** to tip (indexer running).
5. **[run] light contract-held 0/1 seed** (after catch-up; stamp rows with the RPC `latestLedger` so the
   seed's version wins over any older replayed change).
6. **[deploy API + frontend]** — read-cutover, AFTER `balances` is populated (hard coupling: #293 read
   path serves classic/SAC from `balances`; deploying it over empty tables = wrong/empty reads).
7. **[validate]** — incl. measuring frozen (`authorized=false`) magnitude before deciding count-vs-exclude.
8. **[DB] drop `account_balances_current`** — already not written since step 2, so just drop post-validation.

**Blockers before prod (devil's advocate 2026-07-01):**
- **C1** — the `assets.id` ALTER gate (above).
- **C2** — do NOT launch 0331's read-cutover WITHOUT 0339 (or a read-guard): the assets list shows all
  types, so every classic asset renders as a DUPLICATE (type-1 trustlines + type-2 SAC contract-held)
  with divergent supply. Gate 0331 launch on 0339's type-2→type-1 fold, or hide type-2 supply until then.
- **Rollback** — single-write cutover means rolling the indexer back is lossy (the window's account
  updates went to `balances` only); recovery needs a reprocess. Snapshot before the migration/seed.

## CURRENT PLAN 2026-06-29 (karolkow) — Option C unified balances (authoritative)

### Goal
One unified per-holder balance model for ALL asset types (Option C), raw+decimals
everywhere. type-3 was the trigger; the fix is the fundamental balance representation.

### Confirmed decisions
- **Mechanism = ledger STATE** (`ContractData Balance(Address)` entries), NOT event-fold
  (refuted: vault/rebasing drift; 54% non-SEP-41 events — see Findings).
- **Architecture = Option C**: unified `balances(holder_id, asset_id, amount, version)`,
  not separate per-type tables. (Reworks the separate `soroban_token_balances` from the
  earlier commits into this.)
- **Representation = raw `Int128` + `decimals`** (from asset/metadata), scaled at READ.
  Everywhere — classic migrates off `Decimal128(7)`. Universal fixed-point pattern.
- **Holder dimension** — `balances.holder_id` is `cityhash64(holder StrKey)`, one shared surrogate
  space with `accounts.id` / `soroban_contracts.id`. ~~Originally a dedicated `addresses` table.~~
  **Dropped** during cleanup: it was written but never read, and resolution back to a StrKey is
  already available via `accounts` (G) / `soroban_contracts` (C). No separate dimension.
- **Asset surrogate**: add a single `assets.id Int64` (assets is composite-keyed today;
  `balances.asset_id` needs ONE column — mirrors `accounts.id`/`soroban_contracts.id`).
- **Supply = instance `TotalSupply` key** (extend the 0297 instance-storage extraction;
  archival-proof), fallback Σ entries for plain SEP-41. NOT event-sum (vaults drift).
- **Holders = `countIf(amount > 0)`** over `balances`.
- **Seed = RPC snapshot** (holder set from `soroban_events` → batched raw
  `getLedgerEntries`, 99.69% readable incl. archived). **NO ledger reprocess** — proven
  100% by an independent adversarial agent; matches the user's preference.
- **SAC (assets type-2) computed INDEPENDENTLY — IN THIS TASK, but SCHEDULED LAST (step 9),
  spike-gated, does NOT block the core.** (Too many unknowns — struct value, double-count,
  built-in SAC — to gate the core type-3 + unified work on it.) Today both the classic
  (assets type-1) and SAC (assets type-2) rows share
  the one `(code, issuer)` aggregate → identical supply/holders. Instead compute the SAC
  on its own and VERIFY against classic:
  - **holders(SAC)** = classic trustline holders (G-accounts, `account_balances_current`)
    ∪ SAC contract holders (C-addresses, the SAC's `ContractData Balance` entries). **Will
    DIFFER from classic** (adds contract holders). Needs the SAC value decoder (see problems).
  - **supply(SAC)** = the asset's total issuance → SHOULD equal classic trustline sum;
    **verify equality, don't copy**.
  - **Problems to handle (flagged):**
    1. **SAC balance value is a STRUCT** (`BalanceValue{amount,authorized,clawback}`), NOT
       bare `i128` → the type-3 parser (`decode_scval_i128`) skips it. Need a struct decoder.
    2. **Double-count risk on supply**: contract-held balances moved FROM accounts within the
       same fixed issuance — summing trustlines + contract-held double-counts. Correct
       independent supply = the asset total (≈ classic sum); the verify is near-tautological
       unless read from the SAC's own getter.
    3. **SAC is BUILT-IN (no Wasm)** → CLI can't `invoke ... total_supply` by name (spec
       unavailable; confirmed failing). On-chain verification needs a raw-entry/known-key
       spike. This is the part that overlaps 0210 Phase 3 — now pulled into 0331 per decision.
- **Empty tokens** (64% of type-3, no events): `—` is correct, not a coverage gap.

### Gotcha — two different `asset_type` enums (load-bearing)
- `account_balances_current.asset_type` = **Horizon** (0 native, 1 alphanum4 [code ≤4],
  2 alphanum12 [code 5-12]) — verified by code length on prod (type-1 len 1-4, type-2 len 5-12).
- `assets.asset_type` = **project** (0 native, 1 classic_credit, 2 sac, 3 soroban).
- The classic aggregate `WHERE asset_type IN (1,2)` over `account_balances_current` = all
  classic credit (both code lengths) — correct. Do NOT confuse with assets' SAC=2.

### Surrogate keys — confirmed nature (for `assets.id`)
All existing surrogates are `cityhash64` of the natural key: `account_id=hash64(G-strkey)`,
`contract_id=hash64(C-strkey)`, even `transaction_id=hash64(real 32B tx hash)` (a derived
surrogate, not the raw hash). `assets` never had a self-surrogate because nothing referenced
an asset as ONE unit — other tables reference its COMPONENTS (`issuer_id`, `contract_id`).
`balances.asset_id` is the first to need it. Add `assets.id = hash64(canonical identity)`
(native→sentinel; classic→`CODE:ISSUER`+type marker; soroban→contract strkey) — same pattern
as `accounts`. Deterministic = replay-idempotent (the reason hash, not a counter).

### Non-blockers resolved
- **0198 is a POSTGRES task** (Seq Scan on the PG `account_balances_current`,
  `crates/api/src/accounts/queries.rs`), NOT the CH path — **no collision with Option C**.
  Action: verify accounts is served from CH (PG path dead) → **archive 0198 as obsolete**.
- **9% type-3 missing decimals** = 386 tokens with NO on-chain `METADATA` struct
  (non-standard tail) → fall back to 7. Not a backfill gap (data absent).

### Sequence (no 0198 gate) — status as of 2026-06-29

Code for steps 1–7 is on branch `feat/0331` (NOT yet merged / deployed). Remaining:
frontend (step 8), SAC (step 9), and prod run + validation of the seed.

1. ✅ ~~`addresses` dimension~~ — **dropped** (was written but never read; its `(id, strkey, kind)`
   duplicates `accounts` ∪ `soroban_contracts`). Holder→StrKey resolution (for any future
   top-holders / portfolio-StrKey read) is via `accounts` (G-accounts: `accounts.id =
   cityhash64(strkey)`, `accounts.account_id = strkey`) / `soroban_contracts` (C-contracts:
   `.id = cityhash64`, `.contract_id = strkey`) — one shared surrogate space, no dedicated dimension.
2. ✅ `assets.id` surrogate
3. ✅ unified `balances` table (raw+decimals)
4. ✅ type-3 → `balances` (repointed persist off `soroban_token_balances`; dropped that table)
5. ✅ aggregate + read over `balances`
6. ✅ **migrate classic** `account_balances_current` → `balances` — **6a/6b/6c done**;
   **6d (retire legacy classic path) deferred** → see Post-task follow-ups.
7. ✅ **RPC-snapshot seed** bin (`backfill-runner balance-seed`) + `TotalSupply`-key supply
   (commits `f1f0a66f` seed, `1406ea5b` supply). ◻ Remaining: prod run under the catch-up
   gate + ≥10-token on-chain validation (vault + rebasing).
8. ✅ docs (ADR 0032 — clickhouse-pilot §4f + indexing-pipeline §6.2) + **frontend amount rendering
   done** (`scaleByDecimals` in libs/ui): AssetsTable + AssetSummary scale `total_supply`, and
   AccountBalances scales the account portfolio. (All API modules are `ch` in prod — verified
   compute-stack.ts — so the portfolio is raw; PG is dead, so the legacy-pre-scaled concern is moot.
   Also cut the dead PG balance path: `balance-seed` is CH-only, accounts `fetch_balances` PG path removed.)
9. ⏸ **(LAST, spike-gated) SAC type-2 independent** — NOT started. Spike (SAC total_supply vs
   classic + how contract-holders are stored) → if clean: `BalanceValue` struct decoder +
   trustline∪contract holders + verify supply==classic. If messy: defer to 0210/0323. Does NOT block 1-8.
   **Consistency note (load-bearing for step 9):** a balance holder can be a G-account OR a
   C-contract. type-3 ALREADY counts contract-holders in `holder_count` (it reads every
   `Balance(Address)` entry; ~34% of type-3 holders are contracts). Classic (type-1) has NO
   contract holders (trustlines are account-only — a contract can only hold a classic asset via
   the SAC). SAC (type-2) contract-holdings live in the SAC's own `ContractData Balance` entries
   and are NOT yet captured → **until step 9, SAC `holder_count` undercounts (misses contract
   holders), inconsistent with how type-3 counts them.** Step 9 must add SAC C-holders (trustline
   accounts ∪ SAC contract-data holders) to close this cross-type gap.

### Remaining work — DEV-first / OPS-last (re-sequenced 2026-06-30, karolkow)

Decision 2026-06-30: do **all code on branches first**, then batch every prod-run (migrations,
catch-up, seed, validation) into **ONE ops window** at the end. Buys one catch-up wait, ONE
`balance-seed` covering type-3 **and** contract-held 0/1/2, and one validation pass. The
contract-as-holder future-work (Investigation 2026-06-30) is pulled INTO this scope.

**Hard coupling:** PR #293's read-cutover serves classic/SAC from `balances`/`balance_aggregates`,
so it CANNOT be merged to prod without migrations O2a/O2b in the same window (else classic/portfolio
reads go empty/wrong). "Code first" = keep #293 + the new branches **unmerged** until the OPS
window — do NOT merge #293 alone.

#### DEV phase (now, continuous, on branches — no prod-runs except the read-only spike)

- type-3 core (unified `balances`, supply via `TotalSupply`, `balance-seed`, frontend scaling,
  PG-balance cut) — **DONE, on PR #293.**
- **D1. [x] Spike SAC `BalanceValue`** — DONE 2026-07-01 (real mainnet): struct shape
  `{amount,authorized,clawback}` confirmed; decoder validated vs `balance()`/`get_reserves()`/
  StellarExpert (see **Path X** section above). Not messy → SAC leg stays in 0331.
- **D2. [~] Ingest contract-held balances, types 0/1/2** — LIVE parser DONE via **Path X** (above):
  `decode_sac_balance_value` wired into `extract_soroban_token_balances`; keyed by storing-contract
  surrogate (type-2), 0339 folds to type-0/1. Remaining: historical backfill (deferred; same parser via
  a `Run`). Original note below (superseded on keying — no `address_id`/type-1 keying, symmetric instead):
  `ContractData Balance(Address)` scan in each SAC → decode the struct `.amount`
  → write to unified `balances` (raw `i128`), `holder_id = address_id` (Path A, G or C). Fixes the
  type-1/2 supply/holder under-count + contract portfolios.
- **D3. [ ] NFT contract-owner fix** (Path A): `owner_id = address_id` surrogate, union-resolve
  `accounts ∪ soroban_contracts`, drop the `G`/`M`-only guard, while splitting legit `C` owners
  from `L`/`B` false-positives (task 0118). Needs the owner-id schema change.
- **D4. [ ] (optional) `holders` VIEW** — `UNION accounts + soroban_contracts` for one-entity
  joins/endpoints (no synthetic rows).
- **D5. [ ] Surfacing read/UI** — contract detail "holdings" (what a contract holds), rich-list /
  top-holders including contracts, classic/SAC supply+holders now including the contract-held part.
- **D6. [x] 6d code** — DONE: accounts-list native-XLM read is off `account_balances_current`, now a
  PK-prefix key-seek on the unified `balances` table (`queries_ch.rs:168-171`, "task 0331 read-cutover").
  The table DROP stays in OPS, post-validation.
- (optional) dry-run `balance-seed` against stale prod data mid-DEV = cheap pipeline smoke-test.

#### OPS phase (ONE window, at the end — strict order, each gates the next)

- **O1. [ ] Merge all PRs** (#293 + D2–D6 branches).
- **O2. [ ] Migrations:**
  - **a. `assets.id` ALTER + backfill** (`init.sql` ships it `DEFAULT 0`; `CREATE IF NOT EXISTS`
    can't add it). Until backfilled every row has `id=0` → `bagg.asset_id = a.id` and the
    account-portfolio `INNER JOIN assets ON a.id = b.asset_id` match nothing → **empty portfolios**.
    Verify `count(id=0)=0`. (Prereq for BOTH the classic migration AND the contract-held seed.)
  - **b. Migrate classic `account_balances_current` → `balances`** (CH→CH; `Decimal128(7)` → raw
    `Int128` ×10⁷; `holder_id = account_id`; `asset_id = assets.id`). Documented SQL via `chq` (NOT
    a Rust subcommand — the join reads `assets.id` directly, avoiding the Rust `cityhash_102` vs CH
    `cityHash64` mismatch, so do NOT compute the hash in SQL). Mirrors the live 6a dual-write →
    RMT-idempotent, safe to re-run.

    Pre-check (O2a MUST be done first — else every migrated row gets `asset_id = 0`; MUST return 0 or **abort**):

    ```sql
    SELECT count() FROM assets FINAL WHERE asset_type IN (0, 1) AND id = 0
    ```

    Migration (`if(abc.asset_type = 0, 0, 1)` maps Horizon native/alphanum → project native/classic-credit):

    ```sql
    INSERT INTO balances (holder_id, asset_id, amount, last_updated_ledger)
    SELECT abc.account_id, a.id, toInt128(abc.balance * 10000000), abc.last_updated_ledger
    FROM account_balances_current abc FINAL
    INNER JOIN assets a FINAL
       ON a.asset_code = abc.asset_code
      AND a.issuer_id  = abc.issuer_id
      AND a.asset_type = if(abc.asset_type = 0, 0, 1)
    ```
  - **c. NFT owner schema** — apply the D3 owner-id change (discriminator / unified owner).
- **O3. [ ] Ingest catch-up to tip** (the catch-up gate). Was ~12 days / 190,480 ledgers behind on
  2026-06-29 (`max(ledger_sequence)` 63,059,708 vs mainnet 63,250,188). The seed MUST wait for this.
- **O4. [ ] ONE `balance-seed` run** (after catch-up; dry-run first): `backfill-runner --target
  clickhouse balance-seed --soroban-rpc-url <url>`. Seeds type-3 **AND** contract-held 0/1/2 from
  current chain state (freshness-immune to the lag).
- **O5. [ ] Validate everything** vs on-chain getters: ≥10 type-3 incl. vault (MERU) + rebasing
  (EUTBL/eurSAFO); classic (USDC) + account portfolios; SAC type-2 contract-holds; ≥1 contract
  portfolio (e.g. the AMM pool's XLM+EURC, ~1.2M XLM + 194k EURC — see Investigation 2026-06-30);
  NFT contract-owner; holder_count vs independent enumeration on ≥3 tokens; TTL/`removed→0` on a
  dormant holder.
- **O6. [ ] 6d drop** — drop `account_balances_current`, stop the indexer dual-write (ONLY after O5
  passes; CH-internal, NOT 0243-gated — PG is dead).
- **O7. [ ] Feed 0199** — Soroban-LP reserves now live in `balances`; unblocks Soroban-DEX TVL
  (cross-linked in 0199).

#### Faza 3 — lower priority (deferred-in-scope)

- [ ] **Per-protocol decoder for custom-storage Soroban pools** (u32-keyed, e.g. Soroswap/Phoenix):
  LP-share supply + reserves from instance keys. Without it those pools → `—` (honest). Low priority.

#### Enumeration spike (Wątek 2 = B) + decisions (2026-06-30 cont.)

**SAC holder enumeration — event-scan is infeasible (8.3B events, 89% of all soroban_events).** Use STATE, not events:
- **Going-forward (durable):** extend live ingestion to capture `Balance(Address)` ContractData changes — we already process `contract_data` changes for instance keys ([ledger_entry_changes.rs](crates/xdr-parser/src/ledger_entry_changes.rs)). Then every balance change flows live for SAC **and** type-3, no event-scan, no RPC. Reframes even the type-3 event-regex seed.
- **Historical seed:** enumerate `Balance(Address)` per contract from a STATE snapshot — Hubble BigQuery `contract_data_current` (verify Soroban populated) or a history-archive checkpoint. NOT events.

**Decisions (2026-06-30):**
- **W2 = B:** state-snapshot enumeration (above), in the contract-as-holder work.
- **W4 = NO ACTION — NFT contract-ownership ALREADY WORKS** (corrected 2026-06-30; earlier "defer/monitor" and "silent-loss bug" were both wrong — see "Findings (continued)" below). Prod: **2,834 NFTs are contract-owned**, correctly indexed; the bug claim came from reading the **dead PG path**. Live CH (event-derived) captures C-owners fine.
- **W5 = include native:** ingest type-0 (native XLM) contract-held too — a contract's largest holding is often XLM (the pool's 1.2M XLM); excluding it makes contract portfolios wrong.
- **W1 = lean-bundle, W6 = single OPS window** (W1 to revisit). **W3 = deeper research, see plan below.**

**NFT bug magnitude (chq, 2026-06-30):** 12,835 NFTs / 60 contracts; nft_ownership = 12,835 mints + 8,468 transfers + 9 burns; **0 NULL owners on transfers**, the only 9 NULLs are legit burns. → contract-recipient NFTs ≈ 0 today (nascent). Latent bug, not current data loss. No S3 backfill.

## Findings 2026-06-30 (continued) — NFT works / asset-gap real / USDC proof / SAC = facet (0339)

**CORRECTION — NFT contract-ownership ALREADY WORKS** (the earlier "silent-loss bug" was a dead-PG misread):
- Prod CH: **2,834 NFTs are contract-owned** (resolve via `soroban_contracts`, 0 collisions), 9,992
  account-owned, 9 NULL (= genuine burns); **10,934 contract-owner events**, none dropped.
- Why the earlier analysis was wrong: it read the **dead PG path** (`write.rs` sqlx `resolve_opt_id`,
  G/M-only). NFT ownership is **event-derived** — the live CH parser (`nft.rs`, address-agnostic) names
  the owner from the event topics and stores the contract surrogate. C-owners are captured live.
- Action: **none — it works.** Only watch the read/display side: resolve owner via
  `accounts ∪ soroban_contracts`, NOT accounts-only (surrogate ids share one hash space).

**Asset contract-holder gap (types 0/1/2) IS real — NOT the same misread.** `balances` is **not in prod**
(0331 unmerged); fungible contract-held balances are **state-derived** (`ContractData Balance`) and the
parser does NOT extract the SAC `BalanceValue` for 0/1/2 (`state.rs` = trustlines + instance-keys only).
type-3 the parser DOES extract (`extract_soroban_token_balances`), pending 0331 deploy. The difference
from NFT: **event-derived = captured; state-derived = not.**

**USDC empirical proof of the gap (StellarExpert vs us):**

| | Supply | Holders |
|---|---|---|
| us (`asset_aggregates`, trustline-sum) | 202,823,803 | 554,515 |
| SE (Circle USDC) | 250,076,158 | 635,959 funded |
| Δ | **−47.3M (~19%)** | −81k (~13%) |

- Supply **−47M** ≈ contract-held (SAC) + classic-LP reserves + claimable — the venues we don't count
  (+ some 12-day lag). Tangible proof of the contract-as-holder undercount on a real asset.
- Holders **−81k** = both count trustlines (SE doesn't count contracts in "funded") → **staleness /
  trustline-ingest completeness**, NOT the contract gap. Flag for a separate check.

**native vs classic/SAC vs type-3 (StellarExpert contrast):**
- native [XLM](https://stellar.expert/explorer/public/asset/XLM): one asset, `trustlines` funded 9.86M,
  **`contract` = CAS3J7GY (native SAC) as an attribute** → even native uses "one asset + SAC attribute".
- type-3 [Spiko EUTBL](https://stellar.expert/explorer/public/contract/CBGV2QFQBBGEQRUKUMCPO3SZOHDDYO6SCP5CH6TW7EALKVHCXTMWDDOF):
  **metadata only, ZERO supply/holders** (14,255 storage entries unread) → industry-wide gap; **0331 is
  the differentiator** (we read that state).

| | native (0) | classic+SAC (1/2) | soroban (3) |
|---|---|---|---|
| holders | accounts | trustlines + contract (SAC) | ContractData only |
| SE supply/holders | ✅ | ✅ | ❌ "—" |

**SAC = facet of classic (docs-truth, NOT keep-two-rows).** Per official docs ("SAC = an API for
interacting with the asset"; un-deployed SAC = "reserved address, neither asset nor active contract")
+ StellarExpert (one asset, SAC as `contract` attribute, holders = trustlines) → classic + SAC = ONE
asset, ONE supply/holder figure. **Supply/holders for the SAC row vs the classic row SHOULD be EQUAL**
(one asset, shared balances); the new per-`asset_id` `balance_aggregates` would make them diverge into
PARTIALS (type-1 = trustlines, type-2 = contract-held) → that divergence is the bug. **Task 0339**
(backlog on develop — "SAC is a facet of `classic_credit`, drop `asset_type=2`") is the correct
root-fix; **supersedes 0336** (read-collapse band-aid) + **0337** (un-deployed-SAC link guard); it
**overrides the research agent's Option-c "keep two rows".** W3 is now homed in 0339 — coordinate there,
no fresh ADR needed.

## Sequencing correction + checks 2026-06-30 (continued)

**DEV-first / OPS-last stands (W1 = bundle, W6 = single OPS window).** Earlier prose mis-ordered it
(put merge/migrations first) — corrected here.

**Key insight — 0339 is NOT a hard dependency of the SAC leg.** If the SAC/native contract-held leg
writes balances **keyed by the classic/native `asset_id`** (`hash("CODE:issuer")` / `hash("native")` —
both exist today after the `assets.id` backfill), then:
- the numbers are **correct without 0339** (contract-held lands on the type-1 classic row);
- 0339 only **cleans up the duplicate type-2 row** (the 0336 display symptom) — a **separate refactor
  AFTER**, not interleaved, not on the critical path.

So the whole of 0331 stays **one clean DEV→OPS block**; 0339 follows as display cleanup.

- **DEV (all code, branches):** type-3 (done #293) · SAC `BalanceValue` spike (done) · ingest
  contract-held 0/1/2 (state-snapshot — live ContractData changes + Hubble/archive seed; decode
  `.amount`; holder = `address_id`; **keyed by classic/native `asset_id`**, 0339-forward-compatible) ·
  native (W5) · read = trustlines + contract-held on the one classic row · surfacing (contract
  portfolio, rich-list) · 6d code · frozen/deauthorized = count + document. NFT: none (works).
- **OPS (one window, end):** merge all → migrations (`assets.id`, classic→`balances`) → catch-up →
  ONE `balance-seed` (type-3 + contract-held 0/1/2) → validate (incl. a contract portfolio, e.g. the
  ~1.2M-XLM pool) → 6d drop → feed 0199.
- **AFTER, separate:** 0339 (SAC → facet of classic, drop type-2, fold ~31k, supersede 0336/0337).

### Check A — NFT read/display: WORKS ✓
The CH read resolves the owner via **`accounts` ∪ `soroban_contracts`**
(`crates/api/src/nfts/queries_ch.rs:175-181`) and the DTO carries the contract `C…` strkey resolved via
`soroban_contracts` (`dto.rs:18,39`) → a contract owner links to `/contract/`. (The accounts-only join
at `queries.rs:131` is the DEAD PG path.) **Storage + read both correct.** Minor: surrogate hash-space is
shared, so owner-type queries must resolve via `soroban_contracts`, not accounts-only (0 collisions in
practice — the 2,834 contract-owned NFTs have no accounts overlap).

### Check B — USDC holders −81k: staleness + retained zero-rows, NOT ingest incompleteness
| | total trustlines | funded | max ledger |
|---|---|---|---|
| us (`account_balances_current`) | 3,456,365 | 554,733 | 63,192,694 |
| StellarExpert (Circle USDC) | 2,245,437 | 635,959 | (tip) |

- **Total: we have MORE** (+1.2M) → ingest is **not incomplete**; the extra rows are **retained
  closed/zero trustlines** (`removed→0` not pruned — the 0331 "open risk").
- **Funded −81k** = staleness (our snapshot at ledger 63.19M, behind tip) + some funded-on-chain stored
  as 0 in our stale snapshot. **Not missing trustlines.**
- **Data-quality follow-up:** prune zero/closed trustline rows; investigate stale-zero funded balances.

## Research plan — classic/SAC asset-model decision (Wątek 3, 2026-06-30)

> Big schema decision: is `classic X` + `SAC(X)` one asset or two `assets` rows, and how to model supply/holders. Conclusion so far (docs + prod, 2026-06-30): ONE economic asset — one deterministic SAC per asset, account balances **share** the trustline, so one supply = trustlines + contract-balances. But this needs a full research + brainstorm (use the `brainstorming` skill) before any schema change. Devil's-advocate-hardened scope:

**Step 0 — read [0323] (SAC-as-asset depollution) FIRST** — it may already govern the type-1/type-2 row model. Don't brainstorm from blank.

**Must answer:**
1. **Full supply definition** — `Σ trustlines + Σ contract ContractData + Σ classic-LP reserves (+ claimable balances?)`. Trustline-only (and even +contract) is INCOMPLETE: a classic asset in a classic AMM pool sits in `LiquidityPoolEntry`, not a trustline → already excluded today. Define ALL holding venues.
2. **Frozen / deauthorized balances** — does a deauthorized contract balance (`BalanceValue.authorized=false`) / issuer-frozen trustline count toward supply + holder_count? Define, consistently classic vs contract.
3. **Native duality** — native XLM also has a SAC (`CAS3J7GY`); the two-row question applies to type-0 too.
4. **How other indexers model it** — StellarExpert: SAC as a separate asset, or as the classic asset's contract address?
5. **Consumers of the type-2 row** — enumerate every read / FK / join keying on the SAC `assets` row before any collapse / cross-link.
6. **Options + migration reversibility** — (a) two rows + synced metrics; (b) collapse type-2 into an attribute of type-1; (c) two rows, type-2 = "contract view" with no independent supply. Score by correctness AND migration reversibility.

**Output:** a decision + ADR (coordinated with 0323), then the schema/read change. Gates the SAC (type-2) leg of the contract-as-holder work; does NOT gate the type-3 ship.

### Already built (earlier commits — reused / reworked by C)
- `extract_soroban_token_balances` (ContractData Balance parser) — **reused** for live
  holder ingestion.
- `soroban_token_balances` table + persist + `soroban_asset_aggregates` + read coalesce —
  **reworked** into the unified `balances` by steps 3-6.

### Open risks
- **TTL eviction handling**: `removed → 0` must NOT zero an archived-but-positive balance
  (raw reads still return archived values). Verify how eviction appears in LedgerEntryChanges.
- value-shape: bare `i128` confirmed 4/4 type-3; struct shape is SAC (type-2).

### Why not event-fold — re-litigated 2026-06-29 (staleness-IMMUNE proof + external analysis)

An external analysis (Stanisław, via his own Claude session) proposed REVERTING to the original
event-fold (refreshable MV over `soroban_events`, supply = Σmint−Σburn), arguing no backfill is
needed — only ingest catch-up. Re-examined in full. **Verdict: event-fold stays rejected as the
supply source / sole truth; his backfill mechanics are correct but answer the wrong question.**

**Confound found + acknowledged.** The original DECISION refutation numbers (MERU 126.7B vs 63.8B
etc.) compared a STALE fold against CURRENT mainnet: prod CH `soroban_events` is ~190,480 ledgers
/ ~12 days behind tip (measured 2026-06-29: max `ledger_sequence`=63,059,708 vs mainnet 63,250,188).
Those magnitudes are confounded and do NOT by themselves prove the fold wrong (Karol caught this).

**New proof — measures event VOCABULARY, not freshness → unaffected by the lag (prod CH, 2026-06-29):**
- **65.4% of type-3 event volume is fold-blind** — 1,266,280 of 1,936,527 events have NULL or
  non-SEP-41 signatures. Fold keys only on transfer/mint/burn/clawback = 34.6%.
- **179 type-3 tokens emit events but ZERO standard SEP-41** → fold sees nothing → would report
  supply=0 / holders=0 ("—") despite real on-chain activity (~12% of the ~1448 meaningful set).
- **14 tokens emit transfer/burn but ZERO mint** → supply entered via a non-folded path →
  `Σmint−Σburn` is zero/negative while the token circulates → fold is mathematically inconsistent.
- Value-moving events the fold misses, present in the data: `deposit` (30 tokens), `transfer_event`,
  `admn_mnt`, `vault_deposit`/`VaultDeposit`; rebasing/yield: `accrue_interest` (3), `EpochProcessed`,
  `vaulted_event` (balance/supply grows with NO transfer event at all).

**Why fresh data does NOT fix it (deductive).** Catch-up adds more events of the same distribution;
it cannot turn a `deposit` into a `mint`, nor emit a transfer for silent yield accrual. Freshness
fixes the confound (stale-vs-current magnitudes), NOT the structural gap. Fold is correct for
conformant SEP-41 tokens; permanently wrong for vault / rebasing / custom-mint tokens.

**Stanisław's analysis — what's right, what it misses.**
- RIGHT (and useful to US): no historical backfill (0228 filled events from ~50.46M, 9.27B rows —
  verified `min(ledger_sequence)=50,457,424`); **no reparse** — verified in code: the silent-drop
  bug was only in `nft.rs::detect_nft_events`; the generic event stream serializes verbatim
  (`event.rs:143-144`, all topics+data via `scval_to_typed_json`, no shape-drop). His completeness
  proof BENEFITS our seed: we use events only to ENUMERATE holders (G/C addresses in topics), then
  read VALUE from RPC — so the 179 fold-blind tokens are still discoverable by our pipeline.
- MISSES: he answered "does event-fold need a backfill" (no, + catch-up) — NOT "does event-fold
  produce correct numbers." His MV pattern + wording are from the SUPERSEDED event-fold plan, likely
  written without the DECISION/pivot in view. "Wait for catch-up → numbers correct" holds for
  conformant tokens, fails for the 65%-blind volume above.

**Net effect on this task: architecture + sequence UNCHANGED.** Supply = `TotalSupply` key (NOT
fold). Holders = `Balance` entries (event SET + RPC VALUE); fold acceptable only as an
approximation for conformant tokens if ever needed. Reparse open-question now CLOSED (below).

### Step 7 — design + open questions (NEXT; 0 commits; reconstructed 2026-06-29 after session loss)

> The 06-29 research session that produced this was accidentally deleted and is
> unrecoverable. This block + the commit messages are the only surviving record of the
> step-7 reasoning — keep it current so it survives the next session loss.

**Status — core BUILT 2026-06-29 (TDD).** `backfill-runner balance-seed` (CH-only). Pure units
`rpc_snapshot::balance_ledger_key` + `decode_balance_entry` unit-tested (RED→GREEN); orchestration
`balance_seed.rs` (candidate scan → RPC `getLedgerEntries` → decode → upsert `balances`)
mirrors `upgradeable_backfill`; candidate SQL validated on prod CH (MERU = 5974 holder
candidates). Supply = authoritative instance `TotalSupply` key (RPC-probed: i128 on MERU/
USDC-style, absent on plain soroban-sdk → fallback `Σ balances`), written to
`soroban_token_supply` and coalesced over the sum at read. Remaining before prod: run under
the catch-up gate + ≥10-token on-chain validation.

**Scope:** seed existing holders + pick the supply source. The live indexer only captures
NEW `ContractData` changes; step 7 backfills the balances that already exist on-chain.

**Reuse — NOT from scratch.** `crates/backfill-runner/src/rpc_snapshot.rs` (built for the
0320/0326 WASM-upgrade backfill, untouched by 0331) already has the `getLedgerEntries`
`RpcClient` + batching + ledger-key builders (account / trustline / contract_code) + snapshot
decoders. Step 7 = **add** a `Balance(Address)` ContractData key builder + a contract-instance
key builder, **add** an i128-balance decoder + a `TotalSupply` instance-key decoder, then wire
a seed bin on top. Do not rebuild the client.

**Pipeline (from Findings):** holder candidates ← G/C strkeys in the token's `soroban_events`
topics+data → batched raw `getLedgerEntries` on `Balance(addr)` keys → drop zero/absent →
`holder_count` = nonzero count. supply ← instance `TotalSupply` key, else `total_supply()`,
else Σ balances (exact only for non-vault SEP-41). Empty tokens (64%) → `—`.

**Precondition — catch-up gate (sequencing, NOT code).** The seed reads CURRENT mainnet state via
RPC, but the live balance path is fed by the indexer, which is ~12 days / 190,480 ledgers behind
tip (2026-06-29). Run the seed only once the indexer is at ~tip — else the seed is correct at
seed-time but decays stale until the lagging indexer reaches the seed ledger (~2 weeks). Manual
check: `SELECT max(ledger_sequence) FROM soroban_events` ≈ mainnet tip (same gate as the Notes
sequencing item). Optionally a guard in the seed bin refusing to run if the indexer is >N ledgers
behind. Not feature logic.

**Open questions — resolve before/at implementation:**
1. **`TotalSupply` key — RESOLVED 2026-06-29.** RPC-probed across classes: instance-storage
   `Symbol("TotalSupply")` i128 (MERU vault `126,717,554,425,310`; USDC-style: present; plain
   soroban-sdk: ABSENT → `Σ balances` fallback). Built: `instance_ledger_key` +
   `decode_total_supply` → `soroban_token_supply` table, coalesced over `balance_aggregates` at read.
2. **TTL / eviction in `LedgerEntryChanges` still OPEN** (see Open risks) — `removed→0` must not
   zero an archived-but-positive balance. Blocks both live and seed correctness.
3. **Batch cap:** this doc says ≤190 keys/req; `rpc_snapshot.rs` has its own constant — reuse it,
   don't introduce a third number.
4. **Reparse — RESOLVED 2026-06-29: NOT needed.** Verified in code (`event.rs:143-144` serializes
   topics+data verbatim; the silent-drop bug was nft-detector-only, `nft.rs::detect_nft_events`).
   `soroban_events` is a faithful holder-enumeration source; re-decoding recovers nothing. (Also
   confirms no historical event backfill — 0228 filled from ~50.46M, verified.)

## Prior art / why this is a distinct task

- **0194 (completed)** populates `total_supply` + `holder_count` for **asset_type
  1 & 2** (classic credit + SAC) by summing `account_balances_current`
  (`asset_aggregates_mv WHERE asset_type IN (1,2)`). **Type 3 is explicitly not
  covered** — bespoke Soroban tokens have no trustlines.
- **0210 (backlog, classic-parity BUG)** extends classic `total_supply` to 4
  Horizon sources; its **Phase 3 ("SAC contract holdings from `contract_data`")**
  overlaps mechanically — the event-fold here is a **better mechanism for that
  Phase too** (standardised events vs a contract-storage scan). 0210 also puts
  `holder_count` parity **out of scope**; this task owns holders for type 3.
  → cross-linked; a note added to 0210.
- **0138 (archived — SCOPE-OUT)** tried exactly this (Soroban token balances) via
  a **`contract_data` storage scan** and was abandoned because **token storage
  key layouts are non-standard** (per-contract `Balance(addr)` schemas). The
  event path **supersedes that approach** — SEP-41 `transfer`/`mint`/`burn` topics
  ARE standardised, so identification is reliable.

## Context

- Soroban token **balances** live in per-holder **persistent `ContractData`**
  (`Balance(addr)` keys), not in `account_balances_current` — so the existing
  aggregate can't see them. (Token **metadata** lives in the single
  `ScContractInstance.storage` map — that is what 0297/0304 extract; balances are
  a different, sprawling storage tier — hence the asymmetry.)
- SEP-41 events are already ingested **decoded** in `soroban_events` (`topics_xdr`
  / `data_xdr` are tagged JSON, not raw XDR). The NFT side already folds
  `mint`/`transfer`/`burn` the same way (`nft_reparse.rs`).

## Validation (prototype, 2026-06-26)

Event-fold `Σmint − Σ(burn+clawback)` matched on-chain `total_supply()` **exactly**
on every sampled token that exposes the getter: nk_ustry, native-USDC-RAUM-LP,
PPRIME-USDC-SOROSWAP-LP, SMOL, AQUA-XTAR-SOROSWAP-LP (264575131),
LIBRE-oUSD-SOROSWAP-LP (487976433). Method = `simulate total_supply()` via
py-stellar-sdk against mainnet RPC. Several tokens (GIGGLE, NRX, pool shares)
**revert `total_supply()` on-chain** — the fold supplies a number the getter can't,
which is part of the value.

## DECISION 2026-06-29 (karolkow) — PIVOT: ledger-STATE, not event-fold

**The event-fold premise (incl. the "6/6 exact" prototype above and the whole
event-fold Implementation Plan + Analysis below) is REFUTED. Chosen mechanism:
capture per-holder Soroban balances from `ContractData` `Balance(Address)` ledger
entries — the SAME way trustlines feed `account_balances_current` — and aggregate
supply/holders uniformly. Everything below this section is kept as historical trail.**

### Why the fold was refuted (measured on prod via `stellar` RPC, 2026-06-29)

- On-chain `total_supply()` (storage) vs folded, 3/3 tokens with a getter MISMATCH:
  MERU 126.7B vs **63.8B** (fold ×0.5); eurSAFO 39.4B vs **31.3B**; EUTBL 40.8B vs **39.2B**.
  Per-holder also off (MERU balance(GDPY…) chain 800,009,446,178 vs folded 988,468,658,323).
- **54% of type-3 token events have NULL `signature`** (1,027,142 / 1,885,974) — bespoke
  app events, not SEP-41. MERU mints shares via `DeFindexVault/deposit` (amount buried as
  `df_tokens_minted` in a nested map), NOT `mint`. eurSAFO/EUTBL are yield-bearing
  (rebasing) — balances change with **no event at all**. The fold cannot see any of these,
  and it's unbounded: every protocol invents its own event vocabulary.
- The fold reconstructs state from an incomplete history. Reading the ledger entry IS the
  state. The standard `Balance(Address)` ContractData key was confirmed readable on MERU
  (a non-SEP-41 vault token): raw entry value 800,009,446,178 = `balance()` exactly.

### Why this isn't a protocol asymmetry (the inherited assumption, refuted)

- The xdr-parser ALREADY decodes `ContractData` balance entries
  ([ledger_entry_changes.rs:189](../../../crates/xdr-parser/src/ledger_entry_changes.rs)).
  [state.rs:66](../../../crates/xdr-parser/src/state.rs) drops them
  (`if !is_contract_instance_key(&change.key) continue`). The data exists; it's filtered out.
- 0138's "non-standard storage keys" verdict is an **unmeasured assertion** — 0138 was
  actually scoped out for *documentation-scope* reasons (account page = classic-only). The
  standard key works. **Reopen 0138 with the correct reasoning (TTL/archival + doc-scope).**

### Chosen design — option A now, option C later

- **Option A (NOW): parallel `soroban_token_balances (contract_id, holder_id, balance,
  last_updated_ledger)`**, `ReplacingMergeTree(last_updated_ledger)` — a mirror of
  `account_balances_current`, written by a new `extract_soroban_token_balances()` in
  state.rs (filter `key == Vec[Symbol("Balance"), Address]`). Supply/holders aggregate
  = `sum`/`countIf(>0)`. **(As-built correction — see Implementation: a SHARED
  `asset_aggregates` proved impossible — classic is `Decimal128(7)`, type-3 is raw
  `Int128`/arbitrary decimals — so a PARALLEL `soroban_asset_aggregates`, coalesced at
  read, not a shared table.)** Read path mirrors the classic join. Classic hot path
  untouched (`account_balances_current` is mid-perf-surgery under task 0198).
  - **Bonus, ~free:** account portfolio ([fetch_balances](../../../crates/api/src/accounts/queries_ch.rs))
    can finally show Soroban holdings (today invisible); top-holders becomes possible.
  - **Holders span two namespaces:** 34% of type-3 holders are CONTRACTS (C-addresses),
    not G-accounts — hence a parallel table (not shoved into account-keyed
    `account_balances_current`), and the motivation for option C.
- **Option C (FUTURE): unified identity dimension** — see
  [notes/I-unified-identity-dimension.md](notes/I-unified-identity-dimension.md).

### Honest costs / open items for option A

1. **TTL / archival.** Soroban persistent entries archive if TTL lapses → a dormant
   holder's `Balance` entry can vanish from live state → summing entries may under-count
   vs `total_supply()`. Mitigation: read supply from the instance `TotalSupply` storage key
   (always-live, like METADATA which 0297 already extracts) + holders from Balance entries
   (handle restore/archival). **This is the real version of what 0138 half-sensed.**
2. **Backfill.** Seed existing balances: reprocess historical `ContractData` changes
   (backfill-runner) OR RPC-snapshot baselines + live entry-change ingest forward.
3. **Key coverage.** Standard `Balance(Address)` confirmed for MERU; MEASURE % across all
   4,065 type-3 tokens at implementation (most soroban-sdk/OZ conform; quantify the tail).

### Acceptance criteria — SUPERSEDED, rewrite on implementation

The event-fold acceptance criteria below are obsolete. New ACs (storage-state) to be
written: `soroban_token_balances` populated from ContractData Balance entries;
supply/holders match on-chain getters on a ≥10-token sample incl. a vault (MERU) and a
rebasing token (eurSAFO/EUTBL); TTL/archival handled; account portfolio shows Soroban
holdings; docs (ADR 0032) + reopen 0138.

---

## Implementation 2026-06-29 (karolkow) — built, all green, uncommitted

Storage-state path, TDD throughout. Suites green: xdr-parser 280, db-clickhouse 63,
indexer 40, api 43; clippy clean; api-types regenerated + `check-generated` clean.

**Write pipeline:**
- `xdr-parser::extract_soroban_token_balances` — `ContractData` `Balance(Address)` →
  `(contract_id, holder, balance i128, ledger)`; `removed`→0; skips `state` pre-image,
  non-balance keys, and non-bare-i128 values (4 tests).
- Schema `soroban_token_balances` RMT(`last_updated_ledger`); persist `build_soroban_balance_rows`
  (1 test) + `StageInputs`/`StagedLedger`/writer wiring; live indexer (`process.rs` →
  `handler`) + **backfill (`sink.rs`) for free** (shared `ParseOutput`).

**Aggregate + read:**
- `soroban_asset_aggregates` table + refreshable MV (raw `Int128`).
- `assets/queries_ch.rs` (both detail + list): `LEFT JOIN soroban_asset_aggregates ON
  asset_type=3 AND contract_id`; `coalesce` classic+type-3 supply/holders. DTO doc +
  api-types updated.

### Emerged decisions
1. **Parallel `soroban_asset_aggregates`, NOT a shared `asset_aggregates`** — type wall:
   classic `Decimal128(7)` (pre-scaled) vs type-3 raw `Int128`/arbitrary decimals (PIKA
   43224 overflows any Decimal). Coalesce at read instead. Classic path 100% untouched.
2. **`holder` stored as raw StrKey String** (not surrogate) — avoids 0323 one-way-hash
   loss; ~48k rows; display-ready for portfolio/top-holders.
3. **type-3 `total_supply` returned RAW** (frontend scales by `decimals`); classic stays
   pre-scaled. Documented in the DTO.
4. **Supply = `sum(Balance entries)`** for the MVP — see TTL caveat in follow-ups.

### Validation (prod, `stellar` RPC, 2026-06-29)
- Standard `Balance(Address)` key + **bare i128** confirmed on 4/4 sampled type-3 tokens
  (MERU vault, EUTBL, KLT, eurSAFO) — extractor covers them. Struct-value shape is
  SAC-only (type-2, out of scope). Spent-down holders keep a `0`-valued entry (→ the
  `removed`→0 + `balance>0`-filter both matter).

### Follow-ups (not done; spawn on develop)
- **TTL/archival precision** — a dormant holder's `Balance` entry can archive → summing
  entries under-counts vs `total_supply()`. Read supply from the instance `TotalSupply`
  key (always-live, like METADATA/0297); holders from `Balance` entries + restore handling.
- **Deploy + backfill + prod-validate** — `soroban_token_balances` doesn't exist on prod;
  deploy indexer, run backfill (after live catch-up per the Notes gate), validate supply
  vs on-chain getters on ≥10 tokens incl. a vault + a rebasing token.
- **Frontend type-3 amount rendering** (raw → scale by `decimals`) — coordinate with 0257/0304.
- **0210 Phase 3** — SAC `BalanceValue` struct decoder rides this same ingestion (cross-linked).

## Findings 2026-06-29 (two independent agents) — RPC-snapshot seed viable at 100%

Two unbiased agents (no access to this task) audited the data. Net:

**A 100% backfill does NOT require ledger reprocess — an RPC snapshot suffices.**
(Corrects the earlier "reprocess required" assumption.)
- **64% of type-3 (2623/4071) have no events and are EMPTY on-chain** (0 `Balance`
  entries, never minted) → `—` is correct, not a coverage gap. The meaningful set is
  ~1448 (1409 real + 39 oracle/upgrade-only empties).
- **Holder set is fully recoverable from `soroban_events`** (G/C addresses in topics/data
  — even the DeFindex vault emits standard `mint` with the recipient in a topic).
- **Raw `getLedgerEntries` returns values for TTL-archived entries: 642/644 = 99.69%**
  across 40 stale tokens (`liveUntil=0`). `invoke` fails on archived; raw read does not.
- **Supply is best read from the instance `TotalSupply` i128 key** (raw, archival-proof,
  same instance-storage path as METADATA/0297). DeFindex `TotalSupply`=126678419935462 ==
  `total_supply()`. For DeFindex-class vaults, Σ(per-holder events) DRIFTS (`report`/fee
  mints move shares without standard events) — so supply must NOT be summed from events;
  per-holder STATE (entries) + the `TotalSupply` key are correct.

### Refined architecture (post-findings)
- **Holders** ← per-holder `Balance(Address)` entries. LIVE: indexer ContractData parser
  (already built). SEED: RPC snapshot (holder set from events → batched raw
  `getLedgerEntries` ≤190 keys/req → nonzero count).
- **Supply** ← instance `TotalSupply` key (extend the 0297 instance-storage extraction);
  fallback Σ entries for plain SEP-41 without the key. NOT event-sum.
- **Seed = RPC snapshot** (light; no galexie reprocess). Live stays indexer-driven.
- Decimals already on prod (0297): 3840/4222 type-3 have them, **2522 ≠ 7** (variable
  confirmed); ~9% missing → fall back to 7.

### Recovery pipeline (seed, no reprocess)
1. Holder candidates ← scan G/C strkeys in the token's `soroban_events` topics+data.
2. Batched raw `getLedgerEntries` on `Balance(addr)` keys → drop zero/absent →
   `holder_count` = nonzero count.
3. `total_supply` ← instance `TotalSupply` key, else `total_supply()`, else Σ balances
   (exact only for non-vault SEP-41). Empty tokens render `—`.

## Implementation Plan (SUPERSEDED — event-fold; historical trail)

### Event shapes (confirmed)

- `topics_xdr` = JSON array `[{"type":"sym","value":"transfer"},{"type":"address","value":"…"},…]`.
  Event name = element 1 `value`.
- `transfer` (3 topics): `[transfer, from, to]`. `mint`: `[mint, admin, to]` (3) **or**
  `[mint, to]` (2). `burn`: `[burn, from]`. (`clawback` rare.)
- `data_xdr` amount = **two shapes**: `{"type":"i128","value":"<dec>"}` (most) **and**
  `{"type":"map","value":[{"key":{…amount},"value":{"type":"i128","value":"<dec>"}}]}`
  (~29832 mints). Extractor MUST handle both. Use **`Int128` / `Decimal256`** (NOT
  `Decimal128(7)` — overflows on 0-dec / huge-supply tokens, e.g. PIKA decimals=43224).

### Aggregation (CRITICAL construction — see CH gotcha #19b)

- **Supply** = `sumIf(amt, ev='mint') − sumIf(amt, ev IN ('burn','clawback'))` grouped by
  `contract_id` (numeric), **with ZERO joins over the event stream**.
- **Holders** = per-address fold: recipient (`+amt`) = **last** topic for `transfer`/`mint`;
  sender (`−amt`) = topic 2 for `transfer`, last topic for `burn`/`clawback`. Then
  `balance = Σdelta` per `(contract_id, address)`, `holder_count = countIf(balance > 0)`.
- Materialise as `soroban_asset_aggregates {contract_id Int64, total_supply, holder_count}`
  (mirror `asset_aggregates_mv`). Decide refreshable-MV (full recompute, simple) vs
  incremental/scheduled based on `soroban_events` size/cost.
- **Resolve `id → strkey/symbol/decimals` SEPARATELY** at read time on the small
  per-page set (GROUP-BY-collapsed CTE per gotcha #13/#19) — never fold + join in one
  full-table query (re-introduces the RMT ×N row multiplication; see #19b — this is the
  ×100 the prototype hit, not a fold error).

### Read path

- `assets/queries_ch.rs`: for `asset_type = 3`, LEFT JOIN `soroban_asset_aggregates` on
  the numeric `contract_id` over the page set. `AssetRow.total_supply` / `holder_count`
  already exist (currently NULL for Soroban) — just populate.

## Acceptance Criteria (storage-state — rewritten 2026-06-29; replaces the event-fold list)

- [x] Unified `balances` populated from `ContractData Balance(Address)` ledger STATE
      (live parser + the `balance-seed` RPC snapshot) — NOT an event-fold.
- [x] `balance_aggregates_mv` computes `total_supply` (`sum`) + `holder_count`
      (`countIf(amount > 0)`) over `balances`, keyed by `assets.id`.
- [x] type-3 supply read from the authoritative instance `TotalSupply` key, coalesced
      over the sum (`soroban_token_supply`); plain tokens fall back to `Σ balances`.
- [x] Assets list + detail surface supply + holders for `asset_type = 3` (read coalesce).
- [x] type-3 holder_count + supply count BOTH account (`G…`) AND contract (`C…`) holders —
      `balances.holder_id` is a unified surrogate (accounts + soroban_contracts), not accounts-only
      (verified 2026-06-30; ~34% of type-3 holders are contracts). See Investigation 2026-06-30.
- [x] Balance + supply decode unit-tested; non-bare-`i128` (SAC struct) + non-`Balance`
      keys skipped, never mis-summed.
- [x] **Docs (ADR 0032):** `clickhouse-pilot.md §4f` (balance family) + `indexing-pipeline §6.2`
      (balance-seed). **API types:** the `decimals` field was added to the assets + account-balance
      DTOs (steps 1–6, regenerated then); the step-7 read/seed + PG-cut changes produced no further diff.
- [ ] **Prod validation:** run `balance-seed` under the catch-up gate; supply/holders match
      on-chain getters on a ≥10-token sample incl. a vault (MERU) + a rebasing token (EUTBL/eurSAFO).
- [x] **Frontend** raw-amount rendering (`scaleByDecimals` by `decimals`) — AssetsTable + AssetSummary
      (`total_supply`) **and** AccountBalances (account portfolio). PG balance path cut (CH-only).
- [ ] **(deferred)** SAC type-2 independent supply/holders — step 9, spike-gated.

## Investigation 2026-06-30 (karolkow) — contract-as-holder coverage across ALL asset types (whole-project sweep)

> Triggered by the question: "can a *contract* hold a type-2/type-3 asset, and do we
> silently skip that anywhere?" Checked official docs + swept the codebase end-to-end
> (2 agents) + probed mainnet live (`stellar` RPC) + prod stats (`chq`). Partly beyond
> 0331 scope (0331 = type-3); kept here because it confirms 0331 is correct and pins the
> adjacent gaps. Tasks NOT spawned (recorded under Post-task follow-ups).

### Official model (docs-confirmed)

A contract NEVER has a classic account or trustline. It holds any asset as a `ContractData`
entry *inside the token/SAC contract*, keyed by its `Address`:

- native (0) / classic (1) / SAC (2): entry lives in the **SAC**, value =
  `BalanceValue { amount: i128, authorized: bool, clawback: bool }` (a struct).
- soroban token (3): entry lives in the **token**, value = bare `i128`.

`Address` = `Account(G…)` | `Contract(C…)` for all four. Sources:
[SAC docs](https://developers.stellar.org/docs/tokens/stellar-asset-contract),
[CAP-46-06](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-06.md).

### What we actually index (verified, file:line)

| asset type | G-account holder | Contract `C…` holder |
|---|---|---|
| native 0 | ✅ `AccountEntry.balance` | ❌ **skipped** |
| classic 1 | ✅ `TrustLineEntry` | ❌ **skipped** |
| SAC 2 | ✅ `TrustLineEntry` | ❌ **skipped** (`BalanceValue` struct never decoded) |
| soroban 3 | ✅ `balances` (this task) | ✅ `balances` (this task) |

- Ingestion reads `contract_data` only via `is_contract_instance_key` (instance keys —
  discovery/metadata), `xdr-parser/src/state.rs:60,66`. **No `Balance(Address)` ContractData
  decode in ingestion** (only test fixtures match `ScAddress::Contract`).
- `account_balances_current` is trustline/AccountEntry-fed → **G-only**.
- `asset_aggregates_mv` sums `account_balances_current WHERE asset_type IN (1,2)`
  (`db-clickhouse/schema/init.sql:283,298`) → **type-1/2 `total_supply` + `holder_count`
  undercount the contract-held portion.**
- `balance-seed` picks `WHERE asset_type = 3` only (`backfill-runner/src/balance_seed.rs:185`)
  → this task fixes the C-holder case for **type-3 only**, and only bare `i128` (not the SAC struct).
- **NFTs:** contract owner `C…` → NULL (`indexer/.../write.rs:2762` `resolve_opt_id` rejects
  non-`G`/`M`; `.../staging.rs:1491` `is_strkey_account` is G-only); `nfts.current_owner_id` /
  `nft_ownership.owner_id` are accounts(`G`)-only FKs with no discriminator → **an NFT owned by
  a contract is recorded owner-less (silent data loss).**

### Concrete mainnet evidence (live `stellar` RPC, 2026-06-30)

AMM pair `CATUJXDUO7SSSTAKSUV5YU6RSTB4B5AVIHQDV26QTCXOB46T6SLMWNMY` (type-3, custom u32-keyed storage):

- native XLM SAC `balance(pair)` = **12,150,286,124,879 stroops ≈ 1,215,028 XLM**
- EURC SAC `balance(pair)` = **1,939,341,492,641 ≈ 193,934 EURC**

Both real, both held *by the contract*, both **invisible** to our explorer today — one pool
alone ≈ 1.2M XLM + 194k EURC missing from supply/holder math. (Read via SAC `balance()` view,
`--send=no` simulate; confirms types 0 + 2 contract-holdings are real and material.)

### `chq` stats (prod, 2026-06-30)

- 430,188 contracts indexed.
- type-2 SAC: 28,769 assets, only 1,382 (**4.8%**) report any supply (trustline-derived).
- type-3: 4,100 assets, **0%** supply/holders pre-task.

### Other explorers

StellarExpert's contract API returns metadata / storage / events but **no asset balances**
("not yet configured" for Soroban contract holdings). The gap is **industry-wide** — Horizon
doesn't carry contract balances; surfacing them needs RPC/archive STATE reads (exactly the
`balance-seed` mechanism). Doing it is a differentiator, not table stakes.

### Consequences of skipping contract-as-holder

**Assets (types 0/1/2 — type-3 is fixed by this task):**
- **`total_supply` (classic/SAC) undercounts** — it is `Σ trustline balances` only; the
  contract-held amount is missing. If X% of an asset sits in Soroban DeFi, the headline
  supply / "in circulation" is X% too low.
- **`holder_count` undercounts** — contract holders are not counted.
- **Soroban-LP TVL invisible** — a pool's reserves ARE contract-held balances (see LP section)
  → DeFi/TVL analytics blind.
- **Contract detail page incomplete** — "what does this contract hold (XLM, USDC…)?" is blank
  for pools / vaults / bridges; a basic block-explorer expectation.
- **Rich list / top-holders wrong** — the largest holders (pools hold millions) drop off.
- **Account vs contract asymmetry** — a G-account portfolio is complete; a C-contract portfolio
  is empty. Inconsistent UX.
- **Not an old-code bug** — an unhandled post-Soroban case. Trustline-only supply was complete
  pre-contracts; post-Soroban it is incomplete and grows more wrong as DeFi grows. Industry-wide
  (StellarExpert same). Magnitude material (proof: 1.2M XLM + 194k EURC in one pool).

**NFTs:**
- An NFT owned by a contract (marketplace escrow, staking, vault/collateral, DAO/multisig) is
  recorded **owner-less** (`owner_id = NULL`); the owner FK is accounts(`G`)-only with no `G`/`C`
  discriminator, so a contract id cannot even be stored.
- "Who owns this NFT" is blank; "which NFTs does this contract hold" is unqueryable; the
  `nft_ownership` history gets a NULL hole (and a hole if it later moves back to a `G` account).
- **Nuance:** the `G`-only filter intentionally drops `L…`/`B…` false-positive NFTs (task 0118);
  legitimate `C…` owners got lumped into the same bucket. The fix must split `C` (legit) from
  `L`/`B` (junk).
- Magnitude: low today (NFTs nascent on Stellar) — flag, not fire.

### Liquidity pools — how they relate (verified 2026-06-30)

Two distinct kinds, both modeled correctly:

- **Classic AMM LP** (CAP-38, `L…` PoolID): **NOT a contract — correctly.** Modeled as
  `liquidity_pools(pool_id FixedString(32))` + `liquidity_pool_snapshots` + pool-share trustlines
  (`TrustLineAsset::PoolShare`) + `operations_appearances.pool_ids`, extracted from
  `LiquidityPoolEntry`. **Reserves are on-ledger → we have them.** This is what 0199 (LP analytics)
  consumes.
- **Soroban AMM LP** (Soroswap/Phoenix/Aquarius, `C…`): **IS a contract — correctly** (deployed
  WASM). In our DB: `soroban_contracts(contract_type=3)` + the LP-share token in `assets(type-3)`.
  Verified: `CATUJXDU` / `CAS3FL6T` → `is_sac=false, contract_type=3, asset_type=3`. **Reserves
  are contract-held SAC balances → the gap above → we DON'T have them.** The 1.2M XLM read off
  CATUJXDU = its reserve = its TVL.

So it is **NOT a "LP-as-contract" modeling bug** (classic≠contract, Soroban=contract — both right).
The bug is we don't read what the Soroban-LP contract **HOLDS** → Soroban-LP TVL is missing for the
same reason as contract-held balances generally.

**Link to 0199 (LP analytics):** Phase 1/2 = classic TVL (reserves from `LiquidityPoolEntry`,
already indexed). Phase 3 (Soroban-DEX) is gated on the SAC→classic price resolver (0061) — but
that is PRICING; it ALSO needs a way to READ Soroban-pool reserves, which the classic
`LiquidityPoolEntry` path cannot provide. The deferred **SAC ContractData balance ingestion**
follow-up (below) is that prerequisite data path — same mechanism as this task's type-3
`balance-seed`. Knocking out the contract-holder gap unlocks Soroban-LP TVL. (Cross-linked in 0199.)

### What we should do (strategic, knowing all this)

1. **Finish 0331** (type-3 supply/holders — code done; deploy migrations + seed + prod validation).
   No scope change.
2. **Design the deferred SAC ContractData balance ingestion (types 0/1/2) as ONE path serving
   both:** contract-held balances (rich list / contract portfolio / correct supply+holders) AND
   Soroban-LP reserves (0199 TVL). Same `Balance(Address)` key as the type-3 seed; decode the SAC
   struct's `.amount`; holder = `Address` (G or C).
3. **Sequence:** contract-holder ingestion before/with 0199's Soroban side; classic-LP analytics
   proceeds independently (reserves already there).
4. **NFT contract-owner:** separate fix — unified owner id space (accounts + soroban_contracts,
   like `balances.holder_id`) + split `C` from `L`/`B` in the owner guard.
5. **Custom-storage Soroban pools** (u32-keyed): LP-share supply via generic state-read → "—";
   reserves are readable from the instance u32 keys but need a per-protocol decoder — lower priority.

### Holder-model decision (2026-06-30, karolkow) — Path A (unified surrogate), NOT synthetic contract-accounts

Two ways to let a holder/owner be an account (`G`) OR a contract (`C`) were weighed:

- **Path A — CHOSEN.** holder/owner = `address_id(strkey)` surrogate; resolve via `accounts` ∪
  `soroban_contracts`. Already shipped as `balances.holder_id`.
- **Path B — REJECTED.** synthesize an `accounts` row per contract so account-keyed code "just works."

Why A:

- `accounts.id`, `soroban_contracts.id`, `balances.holder_id` are the **same** `hash64(strkey)`
  (`ids.rs:100-105`); `G`/`C` strkeys never collide. So account-shaped joins already resolve off the
  shared surrogate — **no synthetic rows are needed to reuse them** (B's only real selling point is
  already free in A).
- B pollutes `accounts` (every account aggregate must now filter by type), duplicates the id into two
  tables with NULL `G`-only columns (`sequence_number`, `home_domain`), **still** needs a type
  discriminator, **still** needs the ContractData extraction (mandatory in both paths), and pushes
  contract balances into the wrong table (`account_balances_current` is classic `Decimal128(7)`;
  contract-held SAC balances are raw `i128` → belong in `balances`).
- A keeps semantics honest (accounts = G, contracts = C, holder = union only where holding matters)
  and consistent with the shipped `balances` model.
- **Optional ergonomics (not synthetic rows):** a read-only `holders` VIEW
  (`SELECT id, account_id, 'account' … FROM accounts UNION ALL SELECT id, contract_id, 'contract' …
  FROM soroban_contracts`) gives ONE logical holder entity for joins/endpoints — physically Path A,
  logically the convenience of B, with no duplication/sync/pollution.

**Both deferred follow-ups below use Path A** (`address_id` surrogate + union-resolve), never synthetic
contract-accounts. The mandatory ContractData extraction is independent of this choice.

### Verdict / scope

- **0331 is correct for type-3:** holder_count + supply count BOTH `G` and `C` holders (where
  the token uses standard `Balance(Address)` storage; custom-storage pools/oracles → honest "—",
  by decodability not an SEP-41 allowlist — see Why-not-event-fold).
- **types 0/1/2 contract-holdings + NFT contract-owner are separate, out-of-0331 gaps** —
  recorded under Post-task follow-ups (NOT spawned).

## Notes

- **Sequencing — do this AFTER live ingest resumes and catches up (`L_stop` → tip).**
  0331 is read-side (no conflict with ingest running, unlike the 0304 backfill which
  needed it stopped), but the fold reads `soroban_events`: the 0304/0281 window left a
  gap from ~`L_stop`, so deploying the MV before catch-up **under-counts** recent supply.
  The MV also stays fresh only while live ingest keeps writing events. No other blockers
  (assets `asset_type=3` read path already on `ch`).

- Reuses the validated CI check: sample tokens → `simulate total_supply()` (py-stellar-sdk)
  → compare vs the fold. Catches drift from missed/non-standard events.
- Mechanism is **also a candidate for 0210 Phase 3** (SAC contract holdings) — same
  event-fold instead of a `contract_data` scan. Coordinate if both land.

## Analysis 2026-06-29 (karolkow) — event-fold cost/0323 (SUPERSEDED by DECISION above)

> Kept for trail. The unified-MV-over-events design here is obsolete (event-fold refuted —
> see DECISION). The 0323-disjointness and `asset_type=3` scoping findings still hold and
> carry over to the storage-state approach (same type-3 contract set).


**Chosen approach: ONE shared `asset_aggregates` table + ONE refreshable MV, not a
parallel pipeline.** Mirror the disjoint key the `assets` table already uses
(type 1,2 → code+issuer; type 3 → contract_id). The MV body becomes
`balances-branch (asset_type IN (1,2), contract_id=0) UNION ALL event-fold-branch
(asset_type=3 set, code='' issuer=0)`. Read path coalesces two guarded LEFT JOINs to
the SAME table (`asset_type IN (1,2)` on code+issuer; `asset_type=3` on contract_id) —
a 3-col equi-join breaks type-2 (SAC has contract_id in `assets` but the balance row is
keyed code+issuer/contract_id=0). No second table, no second MV, no API-types change
(`total_supply`/`holder_count` already in the response, NULL today).

**Cost — the "is the MV too heavy" Future-Work question is ANSWERED: no.** The 4.37B
`transfer` firehose is the SAC/DEX stream (type-2 / reserved-SAC addresses, see 0323),
NOT the bespoke tokens. Pre-filtering the fold to `contract_id IN (assets WHERE
asset_type=3)` collapses the input to **~647k events** (transfer 457,703 · mint 134,143
· burn 54,934). Measured on prod (`chq`, 2026-06-29): supply fold **0.574s**, holder
fold **1.873s** (1269 tokens, max 5,042 holders). Negligible on the existing 2-min
refreshable MV — same cadence, no incremental machinery needed.

**LOAD-BEARING: the fold MUST stay scoped to `asset_type=3`**, never a broad "any
contract emitting transfer" — a broad fold re-admits the SAC firehose → wrong numbers +
the ×N cost blow-up.

**0323 (SAC-as-asset depollution) does NOT affect this task — verified disjoint.**
0323 operates on SACs (`asset_type=2`): un-deployed SACs move out of `soroban_contracts`
into `assets` as type-2 rows. 0331 folds `asset_type=3` bespoke WASM tokens. Prod check
(2026-06-29): of the 4,065 type-3 assets, **0 are `is_sac=true`, all 4,065 are real WASM
(`contract_type=3`), 0 missing a contract row.** SACs are handled by the balance branch
(`asset_type IN (1,2)`), so 0323 growing the type-2 population never changes the type-3
fold input. Sequencing: disjoint tables (0323 = `soroban_contracts`/`nfts_pending`;
0331 = `soroban_events` read + new aggregate rows) → no new dependency; 0331 keeps its
own "after ingest catch-up" gate.

## Future Work

All open future-work is now folded into **"Remaining work — DEV-first / OPS-last"** above
(DEV/OPS phases + Faza 3), per the 2026-06-30 decision to pull the contract-as-holder analysis
into this scope.

- ~~Refreshable-MV recompute vs incremental~~ **RESOLVED 2026-06-29** — full recompute wins
  (~647k rows / <2s, see Analysis).
- ~~Non-standard / non-conformant token event shapes~~ **MOOT** — Option C reads STATE, not events;
  decode-gated `—` for custom storage (see Why-not-event-fold + Investigation 2026-06-30).

## Post-task follow-ups — PROMOTED into the DEV/OPS roadmap (2026-06-30)

> These are no longer deferred/out-of-scope: per the 2026-06-30 decision they are pulled into this
> task as DEV/OPS steps (6d → O6 + D6; SAC contract-holdings → D2; NFT contract-owner → D3). The
> detailed write-ups below stay for reference.

- **6d — retire legacy classic balance storage.** Steps 6a/6b/6c shipped the classic→unified
  `balances` migration but left the old path in place during transition: `account_balances_current`
  is still dual-written (6a), and the legacy PG `fetch_balances` path is still kept (6c). 6d =
  drop that legacy code/table once the unified `balances` model is prod-validated. Overlaps
  **0243** (PG→CH per-module migration). Deferred to a post-task follow-up; not spawned yet.

- **SAC / native / classic contract-holdings (asset types 0/1/2) — NOT indexed.** A contract
  holding XLM (0), a classic credit (1), or a SAC-wrapped asset (2) stores it as a
  `ContractData Balance(Address) → BalanceValue{amount,…}` entry in the SAC — which ingestion
  never reads (instance keys only). Consequence: `asset_aggregates` `total_supply`/`holder_count`
  for type-1/2 undercount the contract-held portion (proven: one AMM pool holds ≈1.2M XLM + 194k
  EURC, invisible — see Investigation 2026-06-30). **Reframes the deferred "step 9 SAC type-2"**:
  cover 0/1/2 in one path — same `Balance(Address)` key as the type-3 seed, but decode the SAC
  struct's `.amount` (not a bare `i128`), holder = `Address` (G or C) via **Path A** (`address_id`
  surrogate + union-resolve — see Holder-model decision; NOT synthetic contract-accounts). Writes to
  the unified `balances` table (raw `i128`), not `account_balances_current`. Industry-wide gap
  (StellarExpert doesn't show it either). Not spawned.

- **NFT contract-owner — CORRECTED 2026-06-30: NOT a bug, already works** (live CH stores 2,834
  contract-owned NFTs; the NULL claim below is the DEAD PG path, not live behaviour — see
  Findings continued). Original (wrong) write-up kept for trail: ~~An NFT owned by a contract (`C…`) is
  recorded with `owner_id = NULL` (`write.rs:2762` rejects non-`G`/`M`; `staging.rs:1491`
  G-only; owner FK is accounts-only, no discriminator). Independent silent-data-loss bug. Fix via
  **Path A** (owner_id = `address_id` surrogate, union-resolve `accounts` ∪ `soroban_contracts` —
  like `balances.holder_id`; see Holder-model decision) + dropping the `G`/`M`-only guard, while
  still splitting legit `C` owners from `L`/`B` false-positive NFTs (task 0118). Not spawned.~~ **(Moot — NFT contract-ownership already works on live CH.)**

## DECISION 2026-06-30 (karolkow) — `total_supply` = sum-only; DROP the `TotalSupply` key path

Supersedes "Step 7" (the authoritative-key `soroban_token_supply` table + the
`coalesce(tsup, bagg)` read). **One universal method:** `total_supply` = Σ per-holder
`amount` from `balance_aggregates` (the MV-refreshed `sum(amount)` over `balances`).
No second source, no fallback, no seeded key table.

### Why (measured on prod via `chq`, 2026-06-30)
- **4114** type-3 tokens. **2995 (72.8%)** expose a `total_supply()` fn (OZ-family — store the
  on-chain `Symbol("TotalSupply")` instance key); **1119 (27.2%)** do NOT. (Agent first reported
  76.6/23.4 with a looser match; re-measured exact at 72.8/27.2.)
- SEP-41 does **not** mandate `total_supply()` (10-fn interface, none is supply) — verified against
  spec. SAC has no supply view. So the key is an OZ extension, present on ≤73% — **key-only would
  render "—" for ~27%** (option B dead).
- The 1119 no-key tokens are **plain/launchpad fungible tokens, NOT fee-vaults** (all `mint`+`transfer`,
  zero `deposit`/`get_reserves`; top-5 wasm templates = ~60% of the cohort). For a plain token
  `Σ balances` IS the correct supply.
- **Mint always credits a holder balance** (OZ `mint`: `total_supply += x` AND `balance[to] += x`) —
  there is no mint-to-nobody. The "extra"/fee supply of a vault lands on some `Address`, usually a
  **contract treasury (`C…`)**. Under **Path A** we sum **G ∪ C** holders → that balance is captured.
  ⇒ `Σ balances` == real supply, **including** vault/treasury-held supply.

### Accepted non-100% residue (the explicit cost of one method)
- **TTL-archived** balance entries (expired ledger state we can't read) → tiny undercount.
- **True rebasing** tokens (stored `Balance` = shares ≠ effective balance) → rare.
- User decision: not worth a second source / a per-token key read / a refresh cadence. If one specific
  high-value token later reads visibly wrong, add a key read **for that token only** — do not
  reintroduce a blanket `coalesce` (that blend was the original smell).

### Why not keep the key as an override (option C, the prior `coalesce`)
The key table was **seed-only** (`balance-seed` script wrote it once via RPC; the live indexer never
updated it) → it froze at seed time, and `coalesce(tsup, bagg)` preferred that **stale** key over the
fresh 2-min sum. Two columns + a staleness bug for no measurable gain over the sum.

### What was deleted (commit on `feat/0331_…`; −152 net)
- `db-clickhouse/schema/init.sql` — `soroban_token_supply` table → tombstone comment.
- `db-clickhouse/src/lib.rs` — statement-count guard 27 → **26** (24 tables + 1 MV + 1 dict).
- `db-clickhouse/src/persist/rows.rs` — `SorobanTokenSupplyRow` struct.
- `db-clickhouse/src/persist/tests_cross.rs` — its column-order test.
- `api/src/assets/queries_ch.rs` — both builders: `coalesce(tsup, bagg)` → `toString(bagg.total_supply)`;
  removed the `tsup` (`soroban_token_supply`) JOIN.
- `backfill-runner/src/balance_seed.rs` — supply-row decode/insert, instance-key request, `supply_read` stat.
- `backfill-runner/src/rpc_snapshot.rs` — `decode_total_supply` + `instance_ledger_key` (+ their tests).
- `backfill-runner/src/main.rs` — `supply_read` CLI log field.

**Wire shape unchanged:** `total_supply` stays `Option<String>` (RAW `Int128`, client scales by
`decimals`); source-only change ⇒ no openapi / api-types regen. `cargo test` green, zero warnings.
`holder_count` is unaffected — still `balance_aggregates.countIf(amount>0)`.

## STATUS SUMMARY 2026-07-01 (karolkow) — DEV done / DEV left / OPS left

Branch `feat/0331_…` (name is stale — the design is ledger-STATE, not an event-fold).
13 commits `75fe7025..71ee96ff`. All green, pushed.

### ✅ DEV DONE
- **Model:** unified `balances` (holder_id = G∪C surrogate, asset_id, amount `Int128`) +
  `balance_aggregates` MV (`sum(amount)` = supply, `countIf(amount>0)` = holders). Dropped legacy
  `asset_aggregates` view + interim `addresses` table.
- **Supply = Option A (sum-only):** dropped the `TotalSupply` key path (`soroban_token_supply` table,
  key-seed, `coalesce`). Supply is the single `balance_aggregates` sum.
- **Writes = Option B (single-write):** classic + native stage straight into `balances`;
  `account_balances_current` write removed (table KEPT for the migration + rollback). `AssetRow::staged`
  constructor (surrogate `id` computed once) + pinned golden hash test.
- **Reads cut to `balances`:** accounts-list native XLM; account-detail portfolio (incl. type-3 tokens
  with `contract_id` + `symbol` + full `name`, resolved `coalesce(asset_enrichment, metadata)` like
  `/assets`). FE renders the token row (name title + symbol ticker + `/assets/${contract_id}` link).
- **Type-3 ingest wired live + backfill:** `xdr_parser::extract_soroban_token_balances`
  (`process.rs:420` → threaded through `StageInputs.soroban_token_balances`); `balance-seed` RPC-snapshot
  path (funnel stats + unit/integration/**real-mainnet e2e** tests).
- **Cleanup:** dead `assets` columns `name`/`icon_url`/`total_supply`/`holder_count` marked, drop folded
  into task 0310; architecture docs refreshed.

### 🔜 DEV LEFT
- **Contract-held asset types 0/1/2** (a contract holding native / classic / SAC, stored as a SAC
  `Balance(Address) → BalanceValue{amount}` struct) — the reverted SAC-struct leg (`7a5d61d6`).
  Deferred to a **post-0339** task (0339 collapses SAC into classic first). Type-3 contract-holders are
  ALREADY captured (same reader, Path A).

That's it — the type-3 DEV path is complete.

### 🛠️ OPS LEFT (code ready, not deployed)
1. **Merge** PR `feat/0331_…` → develop.
2. **Backfill `assets.id`** (Rust runner — CH `cityHash64` ≠ the Rust surrogate, so SQL can't recompute
   it; existing rows are `id=0` until backfilled → the aggregate JOIN reads `—`).
3. **Classic → `balances` data migration** (copy `account_balances_current`) **+ drop** the old table (6b/6d).
4. **Populate type-3 balances** — pick one:
   - `balance-seed --soroban-rpc-url` — cheap RPC snapshot of event-discovered holders (**~99.7%**;
     misses holders whose token set a balance WITHOUT emitting an event naming them), OR
   - **full re-backfill over the Soroban era from S3** — the live parser already extracts every
     `Balance(Address)` change (see DEV-done wiring), so a historical replay is **100%**, event-independent.
     This is an **OPS run, not new code** — just point the backfill at the historical range.
5. **Catch-up** live ingest.
6. **Validate** — `sum(balances)` vs StellarExpert / spot-check (can't PROVE 100% — chain doesn't
   enumerate holders).
7. *(separate task 0310)* drop the 4 dead `assets` columns in one `ALTER`.

### Leftovers after cutover (checked 2026-07-01)
- `account_balances_current` TABLE — kept on purpose (data + rollback; dropped in 6d).
- No LIVE CH read/write of it remains (one stale bootstrap-test cleanup fixed to `balances`).
- Remaining refs are **PG-dead** (`queries.rs` PG list, PG integration tests — retired with PG, not 0331)
  + `smoke.rs` (exercises the still-existing table) + `audit-harness`/`db-merge` tooling.
