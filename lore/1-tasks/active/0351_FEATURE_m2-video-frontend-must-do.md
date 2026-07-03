---
id: '0351'
title: 'M2 video prep — frontend must-do (eye-catching UX fixes only)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0348']
tags:
  ['frontend', 'ux', 'milestone-2', 'video', 'priority-high', 'effort-medium']
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: karolkow
    note: >
      Curated subset of 0348 — only the eye-catching, high-UX, FE-only fixes
      needed before recording the milestone-2 frontend video. Full root
      causes + file refs live in 0348 (finding numbers referenced as F#).
      Deliberately excludes subtle/backend items (deleted-account, truncation,
      native link, ledger-as-time, etc.).
  - date: 2026-07-03
    status: active
    who: karolkow
    note: Promoted to active — starting M2 video frontend fixes.
---

# M2 video prep — frontend must-do

## Summary

Punch-list for the milestone-2 frontend demo video. FE-only, high visual
impact. Each item cites its 0348 finding (`F#`) — see 0348 for root cause,
file refs, and the fix direction. Goal: nothing on screen looks broken/janky
during the recording.

## Must-do (breaks the demo if left)

Home — first impression:

- [~] **F3 — SKIPPED (not a bug in current code).** No autofocus exists on
  home: `TopNav` (the only `autoFocus` search) is gated off home via
  `{!isHome && <TopNav/>}` (AppShell), and `HeroSearch` is a plain input
  with no autoFocus (never had one). Live: `scrollY=0`, `activeElement=BODY`.
  Home lands at scroll-top, headline visible. Dropped by decision
  (2026-07-03).
- [~] **F17 — SKIPPED (not reproducible; by decision 2026-07-03).** Counters
  use `@number-flow/react` — a purpose-built odometer with digit masking +
  explicit `tabular-nums`. ~2900 measured animation frames across the 4 home
  KPIs showed zero digit garble / zero multi-glyph columns (could not force
  a value roll in-window, so not 100% refuted, but no code-level overlap
  mechanism exists). Same misdiagnosis pattern as F3/F5.
- [~] **F14 — SKIPPED (by decision, 2026-07-03).** Would wire
  `formatCompactAmount` into home KPIs for compact display. Dropped — user
  settled on keeping full US-grouped numbers (compact rejected during F4;
  see memory `feedback_keep_us_number_grouping`). F4 was fixed by wrap, not
  by this cascade.

Core lists / detail:

- [x] **F4 — DONE.** Long supply overflowed into the "Holders" cell. Fixed with
      `overflowWrap: 'anywhere'` on the supply value — it wraps in its cell. US
      thousands grouping kept unchanged (compact/no-grouping both rejected by
      user; see memory). Commit `1dd13b6b`.
- [~] **F5 — SKIPPED (misdiagnosed).** The Time column is NOT clipped — DOM
  shows the full `… UTC` timestamp intact. The table (1100px) is just wider
  than its container on narrow windows → a horizontal scrollbar (`overflow-x:
auto`); content is fully reachable and there is no scroll on wider screens.
  Cosmetic, not data-loss. Dropped by decision (2026-07-03).
- [~] **F11 — SKIPPED (by decision 2026-07-03).** The raw ledger numbers are
  correct (identical only because the top rows are the newest accounts,
  sorted by Last Seen). API exposes no per-account timestamp, so "human
  time" isn't doable FE-only. A "(ledger)" header relabel was tried and
  reverted — dropped.
- [x] **F8 — DONE.** `collection_name` is null for every NFT (0/14696 in CH),
      so the column + its "Filter by collection" input were dead. Both gated
      behind `COLLECTION_COLUMN_ENABLED = false`. Thumbnails left as-is (already
      have image / placeholder / error states; most load). Orphaned collection
      filter caught by `/ux-expert`. Commits `4ea845fc`, `386e1b99`.

## Quick wins (cheap polish)

- [x] **F10 — DONE.** "Any TVL" preset gated behind `TVL_FILTER_ENABLED = false`
      (0341 pattern); asset-pair search stays. Commit `78ef6a64`.
- [ ] **F6 — tables leave an empty void with few rows** before the footer bar —
      fit height to actual row count for small sets.
- [ ] **F7 — NFT trait values oversized** (`heading5SemiBold` 24px) on NFT
      detail — drop to a body variant.
- [x] **F18 — DONE (fixed at source, not FE).** Backend returned the native
      name singular; fixed the shared ClickHouse asset SELECT literal
      `'Stellar Lumen'` → `'Stellar Lumens'` (covers list + detail), plus doc
      mirrors (ADR 0032). No backfill; effective on next Lambda deploy — must
      deploy before recording. Commit `c53e3114`. NOTE: reverses 0161's
      deliberate singular; PG seed/test left singular (PG retired, value never
      served).

## Optional (nice demo feature, not blocking)

- [x] **F19 — DONE.** Added a sun/moon `ThemeToggle` (libs/ui) pinned to the
      far-right of `SecondaryNav` (persistent on every route incl. home) via a
      new `endSlot` prop — the block-explorer convention (Etherscan / Solscan /
      Blockscout / stellar.expert all top-right). Wired to the existing
      `useColorMode().toggleMode`; first-visit default unchanged (system
      `prefers-color-scheme` → dark fallback, already in `readInitialMode`).
      Placement chosen via `/ux-expert`. Verified: light↔dark flips, icon +
      persistence work.

## Explicitly out of scope for the video

Subtle / backend / not eye-catching: deleted-account badge (0349), truncation
standard (F13), native-asset link (F15), search redundant chip (F16),
ledger-sequence-as-time (F12), LP fee de-emphasis (F9), contract invocations
count (F1), native-XLM transactions (F2), API amount nits (0350).

## Acceptance Criteria

- [ ] All "Must-do" items land; nothing on the recorded pages reads as broken
      (done: F4, F8; skipped: F3, F5, F11, F14, F17)
- [ ] Quick wins landed where cheap (done: F10, F18, F7; remaining: F6)
- [x] **Docs updated** — F18 touched the backend read path, so its
      `docs/architecture/**` query mirrors were updated (ADR 0032). The FE-only
      items need no doc change.
- [x] **API types regenerated** — F18 touched `crates/api/**`; regenerated,
      no diff (a query-literal value, not a schema change). Other items FE-only.
