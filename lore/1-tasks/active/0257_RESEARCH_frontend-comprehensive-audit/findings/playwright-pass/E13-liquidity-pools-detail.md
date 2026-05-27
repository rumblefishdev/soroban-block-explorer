# E13 — `/liquidity-pools/:strkey` — Wave 6 Playwright re-pass

H1: `"USDCOIN / EUR"`. Sections (with proper h2/h3): Summary, Pool participants, Recent transactions. Also TVL/Volume/Fees mini-charts.

## Console: 0 errors / 0 warnings on valid.

## Positive verifications — Gate B fix-firsts CONFIRMED

| Fix-first item                            | Status | Evidence                                                                                                                                                                  |
| ----------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **F-L-1** L-strkey URL works              | ✅     | `/liquidity-pools/LD5MMO…O6TL` (composite strkey) renders correctly                                                                                                       |
| **F-K-2** Reserve labels linked           | ✅     | `USDCOIN` + `EUR` in Summary wrapped in `<a href="/assets/USDCOIN-GAFFFRANK…">` and `<a href="/assets/EUR-GAFFFRANK…">`; appears 4× total (header card + Summary section) |
| **F-K-3** "Since ledger" linked           | ✅     | Pool participants table → `Since ledger` values 1,019 and 1,021 wrapped in `<a href="/ledgers/1019">` and `<a href="/ledgers/1021">`                                      |
| **F-D-2** Single NotFound on invalid pool | ✅     | `/liquidity-pools/LXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX` → single "Liquidity pool not found" block, no stacked sub-section error blocks                 |

## Findings

### F-W6-E13-1 [Class A, Severity 🟠 HIGH] Pool participants "Share %" rendered at full precision `33.3333333333333333%`

Snapshot text:

```
GADD…XXXX   600   100.00%        1,019
GAHH…XXXX   200   33.3333333333333333%  1,021
```

100% gets 2-decimal formatting; non-clean fraction gets 16-digit raw BigInt string. Formatter applied inconsistently — perhaps `(shares / total * 100).toFixed(2)` on round numbers but skipped on the second row because the value comes from a `BigInt` arithmetic result not piped through the formatter.

**Severity:** UX-degrading on every fractional share %. Two decimals like `33.33%` is the universal convention.
**Cross-cite:** F-J-12 (Wave 2 number formatting) — likely a missed call-site.

### F-W6-E13-2 [Class A, Severity 🟢 LOW] Pool NotFound has no h1

`h1Count: 0` on the invalid-strkey page. Cross-cite F-W6-NOTFOUND-1.

### F-W6-E13-3 [Class C, Severity 🟢 LOW] Recent transactions section shows operation type as plain text (`Deposit`) without an entity-style chip

The list page (E12) uses no chips; E13 also shows `Deposit` plain. Operation row icons / chip would match the operation-type chips on `/transactions` list.

## Cross-entity exercises

All linked: asset reserve labels (4 occurrences), participants Since ledger (2), participants account (2), Recent tx → tx hash + account (1 each). ✓

## Network requests

`/v1/liquidity-pools/L<strkey>` + participants + transactions sub-section. Same `enabled: !!parentData` gap as E6/E9 if invalid id given — confirmed when navigating to `/liquidity-pools/LXXXX...`: parent 404 + sub-section 404 fire. Cross-cite F-W6-E6-1.
