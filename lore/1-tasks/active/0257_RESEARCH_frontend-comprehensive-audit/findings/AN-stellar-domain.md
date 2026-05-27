# AN — Stellar domain consistency (Wave 4 1.15)

## Per-check verdict table

| Check                                                  | Result | Severity | Evidence                                |
| ------------------------------------------------------ | ------ | -------- | --------------------------------------- |
| Strkey vs hex strategy consistent across display + URL | ⚠      | 🟡       | See F-AN-1                              |
| XDR rendering — where decoded vs raw                   | ✓      | —        | See F-AN-2 inventory                    |
| Operation type → icon mapping consistent               | ⚠      | 🟡       | See F-AN-3                              |
| Asset SEP-1 TOML enrichment handled per-page           | ✓      | —        | See F-AN-4                              |
| Soroban-era ledger detection (>= 50,457,424)           | ✗      | 🟡       | See F-AN-5                              |
| Mainnet vs Testnet config single source                | N/A    | —        | F-AN-6 — single-environment app         |
| Network passphrase usage in FE                         | ✓      | —        | Not used; FE doesn't sign tx            |
| Stroop ↔ XLM conversion central util                   | ⚠      | 🟠       | Per F-U-4 — 2 STROOPS_PER_XLM constants |

## Findings

### F-AN-1 [Class C, Severity 🟡] — Strkey vs hex display strategy inconsistent

**Inventory:**

| Entity         | Stored             | Displayed            | URL             | Util                                                |
| -------------- | ------------------ | -------------------- | --------------- | --------------------------------------------------- |
| Liquidity Pool | hex64 in `pool_id` | SEP-23 `L...` strkey | hex64 (per ADR) | `poolIdHexToStrkey` (web/src/utils/poolIdStrkey.ts) |
| Asset          | numeric `id`       | code or `id`         | `id`            | none                                                |
| Contract       | C-strkey           | C-strkey truncated   | C-strkey        | `truncateMiddle`                                    |
| Account        | G-strkey           | G-strkey truncated   | G-strkey        | `truncateMiddle`                                    |
| Transaction    | hex hash           | hex truncated        | hex full        | local truncation funcs                              |
| Ledger         | numeric sequence   | numeric              | numeric         | none                                                |
| NFT            | numeric `id`       | name                 | `id`            | none                                                |

**Concerns:**

1. **Pool ID display is `L...` strkey, URL is hex.** Most user-visible Stellar canon is SEP-23 strkey, but URL keeps hex (per Wave 1 finding noted in C-api-consistency.md). This is **deliberate** per ADR — but creates a cognitive gap when users paste a `L...` from stellar.expert and expect it to navigate. Search bar partially mitigates (per F-K-4 deferred to Gate B).
2. **Pool strkey conversion does NOT cache.** `poolIdHexToStrkey()` is called per-render in `PoolsTable.tsx:54`, `PoolSummary.tsx:67`, `PoolDetailHeader.tsx:39`. Synchronous, throws on bad input. Cheap (string ops), but a `useMemo` per row in PoolsTable would be cleaner.
3. **No reverse `strkeyToHex` util** for parsing user-pasted pool strkey from search. Per F-L-1 (Gate A deferred), search doesn't recognize `L...` for pools.

**Recommendation Phase 3:** Bidirectional util `poolIdStrkeyToHex(strkey: string): string` + integrate into search routing.

**Class:** C — defer to Gate B with search refactor.

### F-AN-2 [Class A, Severity 🟢] — XDR rendering inventory

**Components:**

- `web/src/pages/transaction-detail/advanced/RawDataSection.tsx` — raw envelope_xdr / result_xdr / results_meta_xdr base64 strings shown via `XdrRow`
- `web/src/pages/transaction-detail/advanced/XdrRow.tsx` — single-XDR display widget
- `web/src/pages/transaction-detail/advanced/HighlightedJson.tsx` — JSON syntax highlighter (for decoded XDR JSON view)
- `web/src/pages/transaction-detail/advanced/OperationJsonDetail.tsx` — operation-level decoded JSON view

**Decoding source of truth:** backend. FE receives both raw `*_xdr` strings (base64) AND structured DTOs (`E3ResponseTransactionDetailLight`, `OperationItem`, `XdrOperationDto`). No client-side XDR parsing (verified: zero imports of `@stellar/stellar-sdk`'s XDR types in `web/src/`).

**Verdict ✓:** Clean separation. Raw shown in Advanced tab; decoded shown in Normal. No client-side XDR parsing means no bundle bloat from `@stellar/stellar-sdk`.

### F-AN-3 [Class A, Severity 🟡] — Operation type → label mapping in 1 place, but icon mapping unclear

**Label mapping:** `web/src/pages/transactions/operationTypes.ts` exports `OPERATION_TYPE_OPTIONS`, `formatOperationType`, `normalizeOperationType`. Single source ✓.

**Icon mapping:** Searching `web/src/` for op-type-keyed icons turns up only `categoryChip(opType: string)` in `web/src/pages/transaction-detail/advanced/OperationJsonDetail.tsx:77` — but this returns a `<Chip>` colored variant, not an icon. Per task README 1.15 spec ("Operation type → icon mapping consistent"), the audit expectation is a per-op icon scheme — not implemented today.

**Verdict:** No icon system; Chips are used uniformly with color variants. Per Figma intent (verify in Wave 6 2.1), this may be deliberate. Document as "label + color chip, no per-op icon system" baseline.

**Class:** A — defer to Gate B Figma check.

### F-AN-4 [Class A, Severity 🟢] — Asset SEP-1 TOML enrichment

- **Hook:** `web/src/api/hooks/useAssetDetail.ts:8` — comment notes TOML metadata included in detail.
- **Consumer:** `web/src/pages/assets/AssetMetadata.tsx:11,57` — renders metadata card with "From TOML" caption.
- **Security note:** `web/src/pages/url.ts:4` comment: "Off-chain metadata (asset SEP-1 TOML) is attacker-controlled" — explicit threat-model awareness. ✓
- **Verdict:** Enrichment handled per-asset on the detail page only (not on lists). Backend serves TOML data; FE renders + escapes. Good.

### F-AN-5 [Class A, Severity 🟡] — Soroban-era ledger detection absent

- **Grep:** `grep -rnE "50_?457_?424|soroban_era|SOROBAN_ERA"` in `web/src/`, `libs/ui/src/` → 0 hits.
- **Implication:** FE does not branch UX based on whether ledger is pre/post Soroban activation (mainnet ledger 50,457,424). On testnet, this would be at a different ledger — currently 0 awareness.
- **Question for product:** is this needed? Pre-Soroban ledgers have no Soroban ops, so the "Invocations / Events" sections on contract detail would naturally be empty. Pre-Soroban transactions wouldn't have `envelope_xdr` Soroban variants. Backend probably returns null/empty for those.
- **Recommendation:** Document explicitly in `lore/3-wiki/frontend-overview.md` whether FE is expected to detect Soroban-era; if yes, add `isSorobanEraLedger(seq: number, network: 'mainnet' | 'testnet')` util. If no, document the assumption.
- **Class:** A — Phase 3 documentation task.

### F-AN-6 [Class D, Severity 🟢] — Mainnet/Testnet config single-environment

- **Grep:** Zero `MAINNET`/`TESTNET`/`NETWORK_PASSPHRASE`/`VITE_NETWORK` references in `web/src/`.
- **Config:** `web/src/api/config.ts` has only `VITE_API_BASE_URL`. App is API-agnostic to network — backend tells FE which network it's serving, but FE doesn't branch.
- **Verdict:** Acceptable for current single-environment dev setup. Will need network-banner / per-network theming for production multi-network. Pre-launch nit.
- **Class:** D — Phase 3 if multi-network ever planned.

### F-AN-7 [Class A, Severity 🟠] — Stroop/XLM conversion in 2 places (recap from F-U-4)

Cross-reference U-component-reuse.md F-U-4. Two `STROOPS_PER_XLM` constants (number + bigint), two `formatFee` paths.

**Recommendation:** Unify in `libs/ui/src/format/stroops.ts` exporting `STROOPS_PER_XLM_BIGINT`, `stroopsToXlmString`, `formatFee`. Migrate consumers.

**Class:** A — Phase 3 (per Gate A defer for "not toggling mid-audit" principle; this is a refactor, not a contract change).

## Summary

7 findings: 0 🔴, 1 🟠 (recap), 3 🟡, 3 🟢.

**Stellar domain hygiene is mostly clean.** The one structural gap is Soroban-era awareness, which may be deliberate; document it. Strkey↔hex strategy is consistent within each entity but lacks bidirectional utility for cross-entity pasting. Op-type label mapping has single source; icon mapping doesn't exist by design.

## Delta-audit pointer

Per Gate A note on FilipDz merge: `transaction-detail/advanced/` XDR patterns reviewed above (F-AN-2). Strkey/hex usage there: pool IDs not present in tx-detail context; contract IDs displayed as C-strkey via `Chip` + `truncateMiddle` ✓. Op-type mapping uses `formatOperationType` from `transactions/operationTypes.ts` consistently ✓ (per F-AN-3 evidence list). Network passphrase still unused (FE doesn't sign).

## Post-Gate-B research finding — strkey convention cross-cutting

### F-AN-8 🟠 HIGH [Class B routing/contract] — Pool ID format is hex-only across backend + URL, anti-convention vs Stellar ecosystem

**Date added:** 2026-05-25 (post-Gate-B during 0264 fix design)

**Trigger:** Investigating 0264 fix correctness (F-L-1 search strkey gap) surfaced systemic convention issue.

**Per-endpoint format inventory (backend `crates/api/src/*/handlers.rs`):**

| Endpoint                      | Accepts                                                                               | Industry standard | Verdict                            |
| ----------------------------- | ------------------------------------------------------------------------------------- | ----------------- | ---------------------------------- |
| `/v1/accounts/:id`            | strkey `G...` (`path::strkey(_, 'G', _)`)                                             | strkey            | ✓ canonical                        |
| `/v1/contracts/:id`           | strkey `C...` (`path::strkey(_, 'C', _)`)                                             | strkey            | ✓ canonical                        |
| `/v1/transactions/:hash`      | hex hash (`path::parse_hash`)                                                         | hex               | ✓ tx hash = hex industry-wide      |
| `/v1/ledgers/:seq`            | numeric                                                                               | numeric           | ✓                                  |
| `/v1/assets/:id`              | polymorphic: numeric `assets.id` OR contract strkey `C...` OR `code-issuer` composite | mixed             | ⚠ polymorphic by design (accepted) |
| `/v1/nfts/:id`                | `parse_nft_id` (TBD)                                                                  | ?                 | ? (verify)                         |
| **`/v1/liquidity-pools/:id`** | **hex 64-lowercase ONLY** (`path::pool_id_hex`)                                       | **strkey L...**   | **❌ outlier vs ecosystem**        |

**Industry convention (Stellar / Soroban):**

- Strkey is canonical human-facing ID per CAP-38 + Stellar SDK. G/C/L/M/T/X/S prefix encodes type.
- Horizon API `/liquidity_pools/<id>` accepts BOTH hex and strkey; returns canonical strkey.
- stellar.expert URLs use canonical forms (strkey-first for accounts/contracts/pools; hex for tx hashes).
- Stellar Lab + Soroban CLI: strkey-first.

**FE side inconsistency** (`web/src/pages/liquidity-pools/PoolsTable.tsx`, `pool-detail/PoolSummary.tsx`):

```
PoolsTable.tsx:
  const strkey = poolIdHexToStrkey(row.pool_id);     // converts to strkey...
  href={routes.pool(row.pool_id)}                    // ...but URL uses hex

PoolSummary.tsx:
  value={poolIdHexToStrkey(pool.pool_id)}            // display = strkey
  href={routes.pool(pool.pool_id)}                   // URL = hex
```

**Consequences:**

- User sees strkey `L...` in UI display + copy button.
- URL bar shows hex (`/liquidity-pools/<64-hex>`).
- User copies strkey display → pastes into our search → **F-L-1 fail** (search backend requires hex).
- User pastes strkey from external explorer (stellar.expert / Horizon) → **search fail**.
- User copies hex URL from our URL bar → works internally, but external context expects strkey.

**Root cause:** original pool implementation (0077) chose hex for backend path because SHA-256 hash is naturally hex (32 bytes raw). Strkey wrapping was added later as display-only enhancement, never wired through to URL routing or backend acceptance.

**Recommended fix (Path A, cross-cutting):**

1. `crates/api/src/common/path.rs` — add `pool_id_or_strkey` validator that accepts hex 64-lower OR strkey `L...` (56 chars base32 with checksum). Convert strkey → hex internally before DB lookup.
2. `crates/api/src/search/queries.rs` — search classifier detects `L...` prefix, dispatches pool lookup.
3. `crates/api/src/liquidity_pools/handlers.rs` — every pool handler uses the new validator; internal conversion at boundary.
4. `web/src/router/routes.ts` — pool URL builder accepts both forms; canonical = strkey going forward (URL bar shows `L...`).
5. `web/src/pages/liquidity-pools/PoolsTable.tsx` + `pool-detail/PoolSummary.tsx` — wire `href` to strkey, not hex.
6. `web/src/pages/LiquidityPoolDetailPage.tsx` — `useParams` accepts both; converts to canonical hex internally before TanStack query key (so cache keys stable).
7. Backwards compatibility: old hex URLs still resolve (validator accepts both forms for ~3 months, then deprecate hex URL form).

**Verify also:**

- `/v1/nfts/:id` accepts strkey? (NFT IDs in Stellar can be SAC contract addresses or other forms — check `parse_nft_id`)
- Asset endpoint polymorphic acceptance is OK by design but worth documenting in API docs

**Severity:** 🟠 HIGH (was 🟡 MEDIUM if only display drift, but cross-explorer-paste fail + URL/display divergence promotes to HIGH).

**Class:** B routing/contract — backend wire format affects routing surface; FE URL canonical form affects external interop.

**Effort:** ~3-5h backend (validator + search classifier + handler boundary conversions) + ~1-2h FE (URL builders + display/href alignment + useParams accepting both forms).

**Phase 3 spawn candidate:** `XXXX_REFACTOR_strkey-canonical-everywhere` — bundles F-L-1 + F-K-4 + F-AN-8 into one cross-cutting refactor. Replaces task 0264's narrower scope.

**Impact on Gate B fix-first 0264:**

- Original 0264 task (FE search preprocess only) is **narrower than the actual problem**.
- Either:
  - (a) **Drop 0264**, spawn broader `XXXX_REFACTOR_strkey-canonical-everywhere` Phase 3 task. Wave 6 records F-L-1 as audit baseline, Phase 3 cluster fix.
  - (b) **Rewrite 0264** to scope = backend Path A canonical fix (broader effort ~3-5h instead of ~1h).
  - (c) Keep 0264 as **partial FE preprocess** (search-input-only), Phase 3 task does the rest cross-cutting.
- User decision required.

## Gate B merge resolution 2026-05-26 — develop @ cdb0c81d (PR #219)

### F-AN-8 — **RESOLVED** in `473de2a2` + `db327f7b` + `863a597a` + `9c3db048` + `4716d5f3`

Strkey canonical convention shipped across the full surface via task 0264 Gate B batch. Effective coverage:

- **Pool endpoint:** `crates/api/src/common/path.rs::pool_id_strkey` validator (strkey-only, returns hex internal); 4 pool handlers consume it; wire response field `pool_id` returns strkey; `cargo test` regression cases for strkey accept + hex reject + garbage reject.
- **FE pool URLs:** `routes.pool(...)` callers pass strkey; `isPoolId` validator updated; `LiquidityPoolDetailPage.tsx` `useParams` consumes strkey.
- **NFT route — bonus refactor:** original task body scoped Phase 8 as "verify NFT canonical" (assumed clean). Post-activation audit + stellar.expert convention check found `/v1/nfts/:i32` numeric DB surrogate. **Upgraded to full route refactor:** `/v1/nfts/:contract_id/:token_id` composite path; `parse_nft_path` validates C-strkey + opaque token_id; `get_nft_detail` + `list_nft_transfers` lookup by composite; `nft_id i32` surrogate kept internal-only (cursor/joins). FE composite path `/nfts/:contractId/:tokenId` consumed by `NftDetailPage`; NFT list rows + cross-entity NFT references updated.
- **Evergreen doc:** `docs/architecture/api/url-conventions.md` created with full per-endpoint table + rationale + ADR-0032 cross-link (`863a597a` restore commit).

**Search portion deferred** — Phase 3 (backend search classifier `L...` decode), Phase 9 (no-op confirm), Phase 10 (FE empty-state hint), plus search output strkey alignment for pool + NFT composite — captured in `future-search-followup` task. F-L-1 + F-K-4 STILL OPEN as of Gate B close. See 0264 archive task body §Issues #5 for deferral rationale (4 in-flight subagent commits reverted to keep batch focused).
