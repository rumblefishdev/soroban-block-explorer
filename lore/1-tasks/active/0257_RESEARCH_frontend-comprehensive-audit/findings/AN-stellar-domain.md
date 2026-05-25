# AN — Stellar domain consistency (Wave 4 1.15)

## Per-check verdict table

| Check | Result | Severity | Evidence |
|---|---|---|---|
| Strkey vs hex strategy consistent across display + URL | ⚠ | 🟡 | See F-AN-1 |
| XDR rendering — where decoded vs raw | ✓ | — | See F-AN-2 inventory |
| Operation type → icon mapping consistent | ⚠ | 🟡 | See F-AN-3 |
| Asset SEP-1 TOML enrichment handled per-page | ✓ | — | See F-AN-4 |
| Soroban-era ledger detection (>= 50,457,424) | ✗ | 🟡 | See F-AN-5 |
| Mainnet vs Testnet config single source | N/A | — | F-AN-6 — single-environment app |
| Network passphrase usage in FE | ✓ | — | Not used; FE doesn't sign tx |
| Stroop ↔ XLM conversion central util | ⚠ | 🟠 | Per F-U-4 — 2 STROOPS_PER_XLM constants |

## Findings

### F-AN-1 [Class C, Severity 🟡] — Strkey vs hex display strategy inconsistent

**Inventory:**

| Entity | Stored | Displayed | URL | Util |
|---|---|---|---|---|
| Liquidity Pool | hex64 in `pool_id` | SEP-23 `L...` strkey | hex64 (per ADR) | `poolIdHexToStrkey` (web/src/utils/poolIdStrkey.ts) |
| Asset | numeric `id` | code or `id` | `id` | none |
| Contract | C-strkey | C-strkey truncated | C-strkey | `truncateMiddle` |
| Account | G-strkey | G-strkey truncated | G-strkey | `truncateMiddle` |
| Transaction | hex hash | hex truncated | hex full | local truncation funcs |
| Ledger | numeric sequence | numeric | numeric | none |
| NFT | numeric `id` | name | `id` | none |

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
