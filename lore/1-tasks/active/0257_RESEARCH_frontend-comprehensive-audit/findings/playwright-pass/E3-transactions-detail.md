# E3 — `/transactions/:hash` — Wave 6 Playwright re-pass

H1: `"Transaction Detail"`. Sections rendered: Summary, Operations (with picker), Signatures.
**Stub is GONE** — A1 (Wave 1 CRITICAL) is RESOLVED. 0070+0071 work landed.

## Console: 0 errors / 0 warnings.

## Findings

### F-W6-E3-1 [Class C, Severity 🟢 LOW] Memo field shows "—" even when transaction has no memo type — could be more semantic

Current rendering: `Memo: —`. Spec idiom is to render "No memo" vs "Memo: <value>"; the em-dash equates "missing" with other "—" used for inner_tx_hash (truly nullable). Pattern inconsistency.

**Cross-cite:** J-5 (formatting depth Wave 2).

### F-W6-E3-2 [Class C, Severity 🟢 LOW] "Normal / Advanced" tab pair has no description

The pair (`Normal | Advanced` near top) appears with no caption. Newcomer doesn't know what "Advanced" reveals (XDR? raw?). Tooltip or one-line hint helps.

### F-W6-E3-3 [Class A, Severity 🟡 MEDIUM] Page-level horiz scroll at mobile because full hash renders in Summary

At 375 viewport the 64-char hash inside Summary cell forces document width 802 → page scrolls horizontally. Same root as F-W6-RESPONSIVE-1.

## Cross-entity exercises

Source account `GAHH…XXXX` is a working link to `/accounts/G…`. In Operation section, "Destination account `GABB…XXXX`" is a working link. ✓

## Invalid-id (`/transactions/INVALID`)

Renders single "Transaction not found" block:
> "We couldn't find anything matching this identifier. Double-check the value and try again. INVALID"

No h1 on NotFound page (heading hierarchy gap). Cross-cite F-W6-NOTFOUND-1.

## Network requests

`/v1/transactions/<hash>` 200; (single call, no sub-section queries on valid). On invalid: `/v1/transactions/INVALID` 404; **no extra sub-section calls fired** (TxDetail does not have sub-sections in tab-style; simpler than account/contract pages).
