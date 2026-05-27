# E9 — `/contracts/:id` — Wave 6 Playwright re-pass

H1: `"Contract"`. Sections: Summary, Interface, Invocations, Events.

## Console: 0 errors / 0 warnings on valid happy path.

## Positive verification — F-D-2 fix CONFIRMED for E9

Invalid id `/contracts/CNONEXISTENT...` renders single NotFound block:
> "Contract not found / We couldn't find anything matching this identifier..."

h1Count: 1 (proper heading hierarchy here, unlike ledger/account 404 — these inconsistent). See F-W6-E9-3 below.

## Findings

### F-W6-E9-1 [Class A, Severity 🟡 MEDIUM] Same partial fix as E6 — sub-section queries still FIRE on 404

Console errors on `/contracts/CNONEXISTENT...`:
```
GET /v1/contracts/CNONEXISTENT...           → 404 (parent)
GET /v1/contracts/CNONEXISTENT.../interface → 404 (sub)
```

Per Wave 1 finding F-D-2 / F-AE-5, the original complaint was 4 stacked error blocks on E9. Wave 6 confirms now only 1 visible block (✅ fix) but the request side still triggers — sub-section hook lacks `enabled: !!parentData` gate. Cross-cite F-W6-E6-1 (same root cause).

### F-W6-E9-2 [Class C, Severity 🟢 LOW] Invocations + Events sections render but no obvious empty-state messaging if zero rows

If contract has no invocations, the section header still renders ("Invocations") but tab/section content is "No public interface" or similar. Verify all 3 sub-section empty states have parallel friendly copy + CTA.

### F-W6-E9-3 [Class A, Severity 🟡 MEDIUM] h1 INCONSISTENT on NotFound across detail routes

| Route | h1 on 404 | Source |
|---|---|---|
| `/accounts/<invalid>` | NONE | F-W6-E6-2 |
| `/ledgers/<invalid>` | NONE | F-W6-E5- |
| `/contracts/<invalid>` | "Contract" present (consistent with breadcrumb) | this finding |
| `/liquidity-pools/<invalid>` | NONE | F-W6-E13- |
| `/transactions/<invalid>` | NONE | F-W6-E3-3 |

So E9 is the *one* page where NotFound preserves the entity-type h1. Either all should preserve, or all should match (likely all-preserve for a11y heading nav). Cross-cite F-W6-NOTFOUND-1.

## Cross-entity exercises

Deployer `GAAL…XXXX` → /accounts/G… ✓.
"Deployed at ledger 1,003" — likely should be `/ledgers/1003` link (not visible from snapshot). Spot-check needed.
