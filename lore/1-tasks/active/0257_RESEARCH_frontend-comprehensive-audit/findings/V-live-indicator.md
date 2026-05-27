# V — Live indicator logic (Wave 6 / 2.3)

## Where in UI is the live/active indicator?

Multiple sites:
1. **Footer**: `<span>All systems operational</span>` — plain text, no health probe.
2. **Header stats strip**: 4 stats (TPS / Ledger / Accounts / Contracts) refreshed every 12s but no LIVE pill on the strip itself.
3. **Home Hero card**: "LIVE" pill next to "Current ledger" stat block.
4. **Home "Latest transactions" header**: "LIVE" pill + "Updated in a moment" text.
5. **Home "Latest Ledgers" header**: "LIVE" pill.

## Findings

### F-W6-V-1 [Class A, Severity 🟠 HIGH] DM-1 RE-CONFIRMED — footer hardcoded; ALL live pills also lack freshness logic

Inspected DOM at all 5 sites:
- Footer "All systems operational" → no aria-live, no JS-bound state, no class/style indicating dynamic value.
- LIVE pills (home, latest-tx, latest-ledger) → no JS state checking last-poll time vs now or vs latest-ledger close-time.
- "Updated in a moment" text → static; doesn't update relative time after poll.

The data on display (latest ledger 1024, closed_at 2026-05-22 11:02:00 UTC) is **5 days stale** as of audit (2026-05-27). All 5 live indicators continue to show green/LIVE/operational despite this. This is the spec'd bug (per task README 2.3 "User explicit: bug already identified — zawsze pokazuje").

**Cross-cite:** DM-1 (Wave 2 `quick-wins-DM-DN-CA.md`). Decision per Gate B: accept baseline, defer Phase 3 spawn `XXXX_FEATURE_footer-status-health-probe` AND `XXXX_FEATURE_live-pill-freshness-logic`.

### F-W6-V-2 [Class C, Severity 🟡 MEDIUM] When backfill runs on historical data, "live" disables — **NOT IMPLEMENTED**

Per task README 2.3 spec: "When backfill runs on historical data, 'live' disables?" — no such logic observed. Backfill state is not surfaced to the FE.

The expected behavior would be: backfill activity → `/v1/network/stats` returns a flag like `is_live: false` or `latest_close_at: <oldish>` → FE detects staleness → LIVE pill becomes IDLE/OFFLINE.

### F-W6-V-3 [Class A, Severity 🟢 LOW] Latest-ledger sequence DOES poll and DOES update (so polling itself works)

`/v1/network/stats` returns latest_ledger 1024 — same value across polls because chain is dormant in this dev env. If chain were producing ledgers, the value would update every 12s. So the data-binding works, only the *liveness-judging* logic is missing.

## Current behavior confirm

✓ Per task spec: always shown. DM-1 stands.

## Recommended Phase 3 spawn

`XXXX_FEATURE_live-and-status-indicator-freshness-logic.md`:
- Adds `useLiveStatus()` hook: compares `latest_close_at` with `now()`; threshold (e.g. < 30s) = LIVE; >30s = STALE; >5min = OFFLINE.
- Single source of truth for footer + all 5 LIVE pill sites.
- Adds `/v1/health` backend endpoint check for footer "All systems operational" → green/yellow/red.
- Effort estimate: ~3-4h FE + ~2h backend for /health.
