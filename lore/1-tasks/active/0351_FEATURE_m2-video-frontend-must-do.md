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

- [ ] **F3 — home loads scrolled ~218px down**, hero headline hidden. Land at
      scroll-top (drop search autofocus / force `scrollTo(0,0)`).
- [ ] **F17 — home stat counters garble mid-animation** (digits overlap during
      the rolling tween).
- [ ] **F14 — wire the compact-number helper** (`formatCompactAmount`) into the
      home KPIs and other large-number sites (home shows `24,620,044` while the
      nav shows `24.6M`). **Cascades — also fixes F4.**

Core lists / detail:

- [ ] **F4 — asset detail: supply value overlaps the "Holders" label**
      (`105,477,412,834.034398Holders`). Fixed by F14 (compact supply).
- [ ] **F5 — Time column clipped** on the transactions list (and ledger-detail
      "transactions in this ledger"). Fix the fixed column widths / overflow.
- [ ] **F11 — accounts list "Last Seen"/"First Seen" show raw ledger numbers,
      identical every row** → reads as broken. Relabel + pair with human time.
- [ ] **F8 — NFT list "Collection" column is `—` every row + broken-image
      thumbnails.** Hide/populate the column + add a letter/identicon fallback.

## Quick wins (cheap polish)

- [ ] **F10 — hide the "Any TVL" filter** in Liquidity Pools (no TVL column
      exists) behind a task-0341-style flag (`const … = false`, ~1 line).
- [ ] **F6 — tables leave an empty void with few rows** before the footer bar —
      fit height to actual row count for small sets.
- [ ] **F7 — NFT trait values oversized** (`heading5SemiBold` 24px) on NFT
      detail — drop to a body variant.
- [ ] **F18 — copy: "Stellar Lumen" → "Stellar Lumens"** on the native asset.

## Optional (nice demo feature, not blocking)

- [ ] **F19 — add a visible theme toggle** (the context has `toggleMode` but no
      button; also explains Chrome-light/Firefox-dark from `prefers-color-scheme`).

## Explicitly out of scope for the video

Subtle / backend / not eye-catching: deleted-account badge (0349), truncation
standard (F13), native-asset link (F15), search redundant chip (F16),
ledger-sequence-as-time (F12), LP fee de-emphasis (F9), contract invocations
count (F1), native-XLM transactions (F2), API amount nits (0350).

## Acceptance Criteria

- [ ] All "Must-do" items land; nothing on the recorded pages reads as broken
- [ ] Quick wins landed where cheap
- [ ] **Docs updated** — N/A (pure FE presentation; no system-shape change)
- [ ] **API types regenerated** — N/A (FE-only)
