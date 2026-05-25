---
id: '0075'
title: 'Frontend: Contract detail page'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0003']
tags: [priority-high, effort-large, layer-frontend-pages]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-19
    status: active
    who: karolkow
    note: 'Promoted to active; starting Contract detail page implementation'
  - date: 2026-05-19
    status: active
    who: karolkow
    note: >
      Implementation complete — in review (PR pending). 11 files
      (10 web + 1 docs), 2 shared components touched (SectionCard,
      ExplorerTable). All acceptance criteria met. 16 spec/Figma/API
      inconsistencies + 10 later issues documented below. No tests
      (web has no test harness). Figma verified 1:1 vs frames
      238:7133 / 250:22577 / 250:24249.
  - date: 2026-05-20
    status: completed
    who: karolkow
    note: >
      Closed. PR #201
      (https://github.com/rumblefishdev/soroban-block-explorer/pull/201) —
      16 files committed (10 web + 1 docs + 2 shared DS components +
      .gitignore + task file). Copilot review addressed: `isRecord`
      tightened to exclude arrays, defensive parser filters unnamed
      entries, React keys switched to `${name}-${index}`. Also swapped
      local `shortId` helper for `truncateMiddle` from `libs/ui`.
      5 follow-ups documented (Contracts list + nav, events count,
      diagnostic events, JSONB schema doc, SAC interface stub). CI
      `web:typecheck` red due to pre-existing develop breakage in
      AppShell.tsx + web/src/search/* (unrelated; fix lives on
      feat/0068, unmerged). No tests added — project has no web test
      harness.
---

# Frontend: Contract detail page

## Summary

Implement the Contract detail page (`/contracts/:contractId`) with summary, interface, invocations, events, and stats. This is the primary developer-facing entrypoint for Soroban contracts and the most Soroban-specific page in the explorer.

## Status: Implementation complete — in review

Code complete, verified (typecheck + lint + visual QA against Figma). PR
pending. Not yet archived — will move to `archive/` after the PR merges.

## Context

The contract page must serve as a comprehensive developer tool for understanding a Soroban contract's metadata, public interface, usage patterns, and event history. It uses four separate API endpoints for independent section fetching. SAC (Stellar Asset Contract) identification must be visually clear.

### API Endpoints Consumed (4 endpoints)

| Endpoint                                  | Purpose                                                       |
| ----------------------------------------- | ------------------------------------------------------------- |
| `GET /contracts/:contract_id`             | Contract metadata: ID, deployer, WASM hash, stats, SAC status |
| `GET /contracts/:contract_id/interface`   | Public function signatures: names, param types, return types  |
| `GET /contracts/:contract_id/invocations` | Paginated list of contract invocations                        |
| `GET /contracts/:contract_id/events`      | Paginated list of contract events                             |

### Contract Summary Fields

| Field              | Display                         | Notes                                                                                 |
| ------------------ | ------------------------------- | ------------------------------------------------------------------------------------- |
| Contract ID        | Full, copyable                  | IdentifierWithCopy (task 0062). Prominent at page top.                                |
| Deployer           | Full, linked to `/accounts/:id` | IdentifierWithCopy (task 0062)                                                        |
| Deployed At Ledger | Linked to `/ledgers/:sequence`  | IdentifierDisplay (task 0062)                                                         |
| WASM Hash          | Full, copyable                  | IdentifierWithCopy (task 0062)                                                        |
| SAC Badge          | Badge if applicable             | "Stellar Asset Contract" badge. Visually clear, materially changes user expectations. |
| Total Invocations  | Integer                         | Stats: total invocation count                                                         |
| Unique Callers     | Integer                         | Stats: unique caller count                                                            |

### Interface Tab: Function Signatures

For each public function:

| Field         | Display               | Notes                               |
| ------------- | --------------------- | ----------------------------------- |
| Function Name | Prominent text        | Primary identifier of the function  |
| Parameters    | Name + type per param | e.g., "amount: i128", "to: Address" |
| Return Type   | Type                  | e.g., "bool", "i128"                |

- Readable format, not raw ABI dump
- Should be understandable by non-authors of the contract
- Separate from invocation/event data

### Invocations Tab: Table Columns

| Column        | Display                              | Notes                         |
| ------------- | ------------------------------------ | ----------------------------- |
| Function Name | Text                                 | Which function was called     |
| Caller        | Truncated, linked to `/accounts/:id` | IdentifierDisplay (task 0062) |
| Status        | Badge (success/failed)               | StatusBadge (task 0063)       |
| Ledger        | Linked to `/ledgers/:sequence`       | IdentifierDisplay (task 0062) |
| Timestamp     | Relative                             | RelativeTimestamp (task 0063) |

- Paginated with cursor-based pagination

### Events Tab: Table Columns

| Column     | Display                        | Notes                                           |
| ---------- | ------------------------------ | ----------------------------------------------- |
| Event Type | Label                          | e.g., "contract", "system"                      |
| Topics     | Array display                  | Topic values                                    |
| Data       | Expandable                     | Event data payload, expandable for large values |
| Ledger     | Linked to `/ledgers/:sequence` | IdentifierDisplay (task 0062)                   |

- Paginated with cursor-based pagination
- Include interpretations when available from the backend

## Implementation Plan

> The original plan referenced `apps/web/src/pages/contract-detail/`. The real
> workspace layout is `web/src/...` — see Inconsistency P15. Plan kept verbatim
> for the record; actual paths are in Implementation Notes.

### Step 1: Contract detail query hooks — four independent query hooks.

### Step 2: Contract summary section — ContractSummary.

### Step 3: Interface tab — ContractInterface.

### Step 4: Invocations tab — ContractInvocations.

### Step 5: Events tab — ContractEvents.

### Step 6: Page composition with tabs — ContractDetailPage.

## Acceptance Criteria

- [x] Summary shows: contract ID (copyable), deployer (linked), deployed at ledger (linked), WASM hash (copyable), SAC badge
- [x] Stats display: invocations + unique callers — **windowed** counts, labelled "· last N days" (see P1)
- [x] Interface tab lists functions with param names, types, return types — syntax-coloured, readable
- [x] Invocations tab: paginated table — **Transaction** column replaces "Function name" (see P2)
- [x] Events tab: paginated table with event type, topics, data, ledger
- [x] Tabs do not cause hard reloads; active tab in URL (`?tab=`)
- [x] All four API endpoints fetched independently (partial failure isolated per `SectionErrorBoundary`)
- [x] SAC badge visually prominent when applicable (accent Chip; verified on a SAC mock)
- [x] Param validation: `C…` format for contractId (`isContractId`)
- [x] 404 state: "Contract not found" (`NotFoundState entity="contract"`)
- [x] Loading skeleton and error states per section

## Implementation Notes

**Actual paths** (`web/`, not `apps/web/` — P15):

- Hooks: `web/src/api/hooks/useContract{Detail,Interface,Invocations,Events}.ts` + 4 exports in `index.ts`.
- Page: `web/src/pages/ContractDetailPage.tsx` (rewritten from `PageStub`).
- Sections: `web/src/pages/contracts/{ContractSummary,ContractInterface,ContractInvocations,ContractEvents}.tsx`.
- `web/src/pages/contracts/interfaceMetadata.ts` — hand-written type + defensive parser for the API's `unknown`-typed `interface_metadata` (the OpenAPI spec does not type the JSONB blob).
- Shared components touched: `web/src/pages/detail/SectionCard.tsx` (header surface), `libs/ui/src/table/ExplorerTable.tsx` (header surface) — DS-correctness fixes, see L4. Affect every detail page / table.
- Docs: `docs/architecture/frontend/frontend-overview.md` §6.10 + route table (ADR 0032 evergreen requirement).
- Dev-only, **gitignored**: `web/mock-server.mjs` — zero-dep Node mock API for local preview (no test harness exists).

**Verification:** `nx run web:typecheck` + `lint` green. Visual QA via Playwright CLI + Claude Preview against Figma frames 238:7133 (interface), 250:22577 (invocations), 250:24249 (events), incl. SAC variant. No unit tests — the `web` app has no test target/harness.

## Design Decisions

### From Plan

1. **Four independent query hooks**, per-section `SectionErrorBoundary` — partial failure never collapses the page.
2. **Tabs with active tab in URL** — `useTabUrlState` (`?tab=`), no hard reload.
3. **`C…` param validation + 404** — `isContractId` guard, `NotFoundState`.

### Emerged

4. **Stats labelled windowed, not "total" (P1)** — `ContractStats` exposes `recent_invocations` / `recent_unique_callers` / `stats_window`; no full-history total exists (deliberate — full scan rejected, task 0167). Cards read "· last {stats_window}".
5. **Invocations "Transaction" column, not "Function" (P2)** — the appearance index carries no per-call function name (ADR 0034 — call detail is XDR-only). Transaction hash takes the column.
6. **`interface_metadata` hand-typed (P3, L2)** — API types it `unknown`. Real shape taken from the indexer (`crates/indexer/.../staging.rs`): `{ functions:[{name,doc,inputs:[{name,type_name}],outputs:[]}], wasm_byte_len }`. The research-0003 design doc shape was NOT what got implemented. Parser is fully defensive.
7. **No tab count pills (P4)** — Figma shows counts on Invocations/Events; no event count exists in the API. Pills removed entirely rather than fake or half-populate.
8. **SAC badge improvised (P5)** — accent-yellow `Chip` "Stellar Asset Contract" beside the title; SAC interface tab → "No public interface" empty state. Figma has no SAC frame — pending designer review.
9. **Generic loading/error/empty states (P6)** — Figma has no state frames; reused DS generics (`CardSkeleton`, `TableSkeleton`, `Generic/RateLimit/TransientErrorState`, `EmptyState`, `NotFoundState`) like every other detail page.
10. **Breadcrumb "Contract" (P12)** — Figma frame shows "Account /…" (copy-paste leftover from the Account template); used the correct entity.
11. **Surface 4-tone scheme (L4)** — `#0f0f0f` table header, `#1a1a1a` card headers / tab bar / interface code block, `#212121` page, `#272727` card body. Fixed `SectionCard` + `ExplorerTable` (shared) to match the Figma DS "Table component".
12. **Events type-badge colours from Figma (L6)** — `contract`→blue, `system`→**brown** (not emerald), `diagnostic`→grey.

## Spec / Figma / API Inconsistencies

Full log of every mismatch found this session — between the task spec, the
Figma design, the architecture docs, and the actual API. Recorded so future
sessions do not re-discover them.

| #   | Inconsistency                                                                                            | Source of truth                   | Resolution                                                                |
| --- | -------------------------------------------------------------------------------------------------------- | --------------------------------- | ------------------------------------------------------------------------- |
| P1  | Spec + docs say "total invocations / unique callers"; API gives **windowed** `recent_*` + `stats_window` | API (`ContractStats`), SQL doc 11 | Cards labelled "· last 7 days"; docs §6.10 corrected                      |
| P2  | Spec/Figma "Function" column; appearance index has **no per-call function name**                         | ADR 0034 (XDR-only)               | Column → "Transaction" (hash link); docs §6.10 corrected                  |
| P3  | `interface_metadata` JSONB shape undocumented (SQL doc 12 "SCHEMA-DOC GAP")                              | indexer `staging.rs`              | Shape read from indexer code; hand-typed + defensive parse                |
| P4  | Figma tab bar shows count pills (Invocations 1284 / Events 342); no event count in API                   | API                               | All tab count pills removed                                               |
| P5  | SAC variant + null-field states not in Figma                                                             | task spec (badge required)        | Improvised accent Chip + empty interface state                            |
| P6  | No loading/error/empty Figma frames                                                                      | —                                 | Reused DS generic states                                                  |
| P7  | Spec "include interpretations" for events; no interpretation field in API                                | API (`EventItem`)                 | Dropped                                                                   |
| P8  | Spec "expandable data" for events; Figma renders data inline                                             | Figma                             | Inline mono cell, truncated, full value on hover                          |
| P9  | `frontend-overview` route table listed only 2 of 4 contract endpoints                                    | openapi                           | Doc route table corrected                                                 |
| P10 | `event_type` value set undocumented                                                                      | backend                           | Confirmed `contract`/`system` only (see L1)                               |
| P11 | Events pagination — a page's `data.len()` can exceed `limit` (appearance expansion)                      | API note                          | Pager is cursor-driven, never derives counts from page size               |
| P12 | Figma breadcrumb says "Account" on the contract page                                                     | —                                 | Used "Contract"                                                           |
| P13 | API returns `contract_type`, `contract_type_name`, `wasm_uploaded_at_ledger` — unused by spec/Figma      | Figma                             | Omitted (Figma-first)                                                     |
| P14 | Dep task 0063 still `active`                                                                             | —                                 | Components (`RelativeTimestamp` etc.) already built + usable; non-blocker |
| P15 | Spec paths `apps/web/...`; real workspace is `web/...`                                                   | repo                              | Used real paths                                                           |
| P16 | No mobile / responsive Figma frame                                                                       | —                                 | Desktop-first, consistent with other pages                                |

### Later issues (discovered during implementation / review)

| #   | Issue                                                                                                                                                                                                         | Resolution                                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| L1  | `/contracts/:id/events` never returns `diagnostic` events — the diagnostic XDR container is dropped server-side (task 0182, mirror-leak fix). Figma shows a "Diagnostic" badge that can never render.         | Type badge supports `contract`/`system`; `diagnostic` mapped defensively. Open question for backend (not spawned per request). |
| L2  | Real `interface_metadata` shape ≠ research-0003 design doc — `functions` is top-level, param type field is `type_name` (not `type`), `outputs` is an array.                                                   | Hand-typed to the actual shape.                                                                                                |
| L3  | Tab pills — initial impl had a pill on Interface only (inverse of Figma).                                                                                                                                     | All pills removed (P4).                                                                                                        |
| L4  | Backgrounds initially uniform — Figma uses a 4-tone surface scheme.                                                                                                                                           | Fixed `SectionCard` header, tab bar, interface code block, `ExplorerTable` header.                                             |
| L5  | Interface tab initially: collapsed-by-default, chevron right, uncoloured types. Figma: expanded, chevron left, syntax-coloured.                                                                               | Fixed — `defaultExpanded`, `flexDirection: row-reverse`, `typeColor()`.                                                        |
| L6  | Events `system` badge was emerald; Figma is brown.                                                                                                                                                            | `system` → `brown`.                                                                                                            |
| L7  | No way to reach the contract page without an id — no "Contracts" nav link, no `/contracts` list page. Figma navbar shows "Contracts".                                                                         | Out of scope for 0075 (detail page only). See Future Work.                                                                     |
| L8  | This worktree has no `node_modules`; Node resolves up-tree to the main repo, so `libs/ui` is served from the main repo's source — `libs/ui` edits made in the worktree are invisible in the worktree preview. | Verified `libs/ui` changes by temporarily copying into the main repo's source (reverted after).                                |
| L9  | Process error — `typecheck`/`lint` were run with `cd` into the main repo, so they checked develop's code (false-green); some impl files were initially written to the main worktree.                          | Corrected — files relocated to the worktree; checks re-run in the worktree. See memory `feedback-worktree-paths`.              |
| L10 | The Figma dev-mode MCP connector went down mid-session (`net::ERR_FAILED` on every call).                                                                                                                     | Switched to the official `figma` HTTP MCP via OAuth.                                                                           |

## Issues Encountered

- **Worktree without `node_modules` (L8, L9)** — the biggest time sink. Commands and module resolution silently used the main repo. Root cause: worktree relies on up-tree resolution. Fix: always run `nx` in the worktree (no `cd`), and treat `libs/ui` preview changes specially. Captured in memory.
- **Figma MCP outage (L10)** — `net::ERR_FAILED` on the dev-mode connector; not auth, not a bad node id. Recovered via the official `figma` MCP OAuth flow.
- **No frontend test harness** — `web` has no `test` target and zero test files; consistent with the project. `interfaceMetadata.ts` (pure, defensive) would be a good unit-test candidate if a harness is added.

## Modified Tests

None — the `web` app has no test harness.

## Future Work

Not spawned as backlog tasks yet (awaiting owner confirmation):

1. **Contracts list page + nav link** — `/contracts` list and a "Contracts" entry in `NAV_LINKS`. Without them the contract detail page is unreachable by browsing (L7). Figma navbar already shows "Contracts".
2. **Backend: events count** — add a count to `ContractStats` (or the events endpoint) so the Events tab can show an honest count pill (P4).
3. **Backend question: diagnostic events** — decide whether `/contracts/:id/events` should expose genuine `diagnostic`-typed events (L1). Owner asked not to spawn a task for this.
4. **Document the `wasm_interface_metadata` JSONB shape** — close the SQL-doc-12 "SCHEMA-DOC GAP" (P3) so the frontend type is not reverse-engineered from indexer code. When this lands (preferred path: add a serialized Rust struct + utoipa schema so the OpenAPI types the field properly and codegen produces the TS automatically), `web/src/pages/contracts/interfaceMetadata.ts` becomes redundant — the hand-written types delete and the defensive parser shrinks to a thin runtime guard (or disappears if generated types are tight).
5. **Synthesized SAC interface stub** — for `is_sac = true`, the API currently returns `interface_metadata = null` and the frontend renders an empty state. SACs always expose the standard token interface (SEP-41: `transfer`, `balance`, `mint`, `allowance`, `approve`, `decimals`, `name`, `symbol`). SQL doc 12 explicitly leaves room for this: _"the API translates to 'no interface declared' or to a synthesized SAC interface stub"_. Backend could synthesize the SEP-41 stub when `is_sac` so users see the standard surface; frontend would render it like any other interface (no UI change required).

## Notes

- This is the most Soroban-specific page. It must work as a developer tool.
- The interface tab is especially important: it should make contract APIs understandable without reading source code.
- SAC identification materially changes user expectations (it represents a wrapped classic asset, not a custom contract).
- Four independent queries allow the page to render progressively and degrade gracefully.
- Relevant ADRs (not modified): 0032 (evergreen docs), 0033/0034 (events/invocations appearance index, read-time detail), 0037 (contracts schema), 0042 (contract `name` column), 0182 (diagnostic-events container fix).
