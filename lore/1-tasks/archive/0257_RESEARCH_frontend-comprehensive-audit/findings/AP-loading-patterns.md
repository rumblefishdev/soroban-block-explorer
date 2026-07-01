# AP — Loading patterns (Wave 6 / 2.2b)

## Findings

### F-W6-AP-1 [Class C, Severity 🟡 MEDIUM] Loading pattern inconsistency: skeleton vs spinner choice not codified

Observed via fast-throttle simulation (Playwright network conditions not used; observed during initial load):

- List pages (`/transactions`, `/ledgers`, `/assets`, `/nfts`, `/liquidity-pools`): brief blank-state, then full-table render. No skeleton row pattern.
- Detail pages: same — brief blank, then full render.
- Home: brief blank, then 4 sections paint roughly together.

There's no `<TableSkeleton>` / `<SectionSkeleton>` shared primitive in evidence. Pattern: rely on TanStack `placeholderData: keepPreviousData` + initial blank — works for in-app navigation but for **cold load** (no cache), there's a visible "blank shell + footer" state for ~200-400ms.

**Cross-cite:** F-U-2 (Wave 4 EmptyState reimplemented locally per page) — same root-cause-cluster: shared state primitives not consolidated.

### F-W6-AP-2 [Class C, Severity 🟢 LOW] Polling refresh is SILENT (no visual indicator)

Home tx + ledger tables refresh every 12s; "LIVE" badge + "Updated in a moment" text both static (badge always green; text persists). No micro-pulse, no flash on row append.

Pattern depends on team intent. Some block explorers (stellar.expert) flash new rows; others (etherscan) silently update. Document the choice.

**Cross-cite:** F-V-1 (live indicator broader pattern).

### F-W6-AP-3 [Class C, Severity 🟢 LOW] Error retry has no distinct "retrying" state separate from initial

When a query errors and is automatically refetched (`retry: 3` default in TanStack), the user sees the error block then the spinner-or-blank then content. No "Retrying… (2/3)" copy. Most users won't notice; advanced/QA testing benefits from explicit retry state.

### F-W6-AP-4 [Class C, Severity 🟢 LOW] Inline vs overlay vs full-page loading not standardised

Some sections show inline loading (small spinner), some go blank, some flash content. No single `<LoadingState variant="inline"|"overlay"|"full">` primitive observed.

**Cross-cite:** F-U-2 + F-W6-AP-1; bundle Phase 3.

## Summary

4 LOW-MEDIUM findings — all defer to Phase 3 visual-polish + state-primitive consolidation batch. No fix-first justified.
