# E5 — `/ledgers/:seq` — Wave 6 Playwright re-pass

H1: `"Ledger 1,024"`. Sections: Summary, "Transactions in this ledger" embedded list with pagination.

## Console: 0 errors / 0 warnings.

## Findings

### F-W6-E5-1 [Class C, Severity 🟢 LOW] "Prev ledger" / "Next ledger" buttons present in header but no visual disabled state for boundary ledgers

At `/ledgers/1024` (current head): "Next ledger" still appears clickable. Spot-check on next-ledger boundary needed; likely lands on 404 since seq 1025 doesn't yet exist.

## Invalid-id (`/ledgers/9999999999`)

Renders single "Ledger not found" block. No h1 element (heading-hierarchy gap). Cross-cite F-W6-NOTFOUND-1.

## Cross-entity exercises

Embedded tx rows: hash → `/transactions/<hash>` ✓, source account → `/accounts/G…` ✓.

## Network requests

`/v1/ledgers/1024` + `/v1/ledgers/1024/transactions?limit=20` — both 200. On invalid: parent 404, sub-section 404 also fires (extra console error, even though render is gated single-block). See cross-cite F-W6-NOTFOUND-2.
