# E1 — Home `/` — Wave 6 Playwright re-pass

## Snapshot summary

H1: `"Soroban - first.Stellar - complete."` (concatenated 2 spans into single h1 — fine).
Sections rendered: HeroSearch, Network stats strip, Latest transactions (10), Latest Ledgers (10). LIVE badge present on tx + ledger sections.

## Network audit

Polling: `/v1/network/stats` (12s), `/v1/transactions?limit=10` (12s), `/v1/ledgers?limit=10` (12s). All 200 OK. Polling cadence matches `web/src/api/polling.ts:5` `homePolicy`.

## Console

Errors: 1 (favicon 404 — benign).

## Findings

### F-W6-E1-1 [Class C, Severity 🟡 MEDIUM] LIVE badge on Latest transactions / Ledgers shown always

The "LIVE" pill on home sections (and in HeroSearch + header) renders regardless of actual freshness — same DM-1 pattern at finer granularity. The home tables display data with timestamp `2026-05-22` while current date is `2026-05-27` (= 5 days stale on testnet). User sees "LIVE" + "Updated in a moment" while data is 5 days old.

**Cross-cite:** DM-1; F-V-1 (2.3).

### F-W6-E1-2 [Class C, Severity 🟢 LOW] Hero search box and header search box are visually identical but separate state

Two search inputs visible on home (one in header for "CTRL + F", one in hero for "CTRL + K"). Different shortcut hints, different surrounding styles. Different React state (typing in one does NOT mirror to other). Confusing UX; opportunity to use one shared `SearchInput` controlled-uncontrolled instance.

**Cross-cite:** F-U-2 (Wave 4 component-reuse).

### F-W6-E1-3 [Class A, Severity 🟢 LOW] Home stats strip duplicated in header

`<header>` shows TPS / Ledger / Accounts / Contracts (4 stats); `<main>` hero card also shows Current ledger / TPS / Accounts / Contracts (same 4). Two `<HeaderStatsStrip>`-shaped components subscribe to the same `/network/stats` endpoint with different cache keys (or shared key with N consumers). On desktop both render; on mobile they stack identically. Duplicate visual + duplicate network read.

**Cross-cite:** F-AI-1 (bundle), F-U-2 (component reuse).

## Cross-entity link audit (sampled 5 of each kind from this page)

| Source cell | Destination link | Working? |
|---|---|---|
| Tx hash row 1 | `/transactions/7b9bac…2089` | ✓ (full hash href) |
| Source account row 1 | `/accounts/GAHH…XXXX` | ✓ |
| Ledger 1024 row | `/ledgers/1024` | ✓ |
| Ledger hash row | (no link — display only) | ⚠ inconsistent with tx hash being clickable |
| "View All" tx link | `/transactions` | ✓ |
| "View All" ledger link | `/ledgers` | ✓ |

### F-W6-E1-4 [Class C, Severity 🟡 MEDIUM] Ledger hash on home table is NOT a link, but ledger sequence IS

Inconsistency: row "1024" → links to `/ledgers/1024`. Row "360c…ac1a" (the ledger hash) → plain text. On the tx table, the hash IS the link. Either both should link, or pick one consistently across all tables. Pick = ledger hash isn't a route param so it shouldn't be a link → fine, but consistency note: tx table uses hash for the row-link; ledger table uses sequence. User has to learn each table's convention.

**Cross-cite:** K-cross-entity-links Wave 3.
