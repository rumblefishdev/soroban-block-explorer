# E8 — `/assets/:id` — Wave 6 Playwright re-pass

H1: `"USDCOIN"`. Sections: Summary (Issuer, Total supply, Holders), Metadata (From TOML), Latest transactions.

## Console: 0 errors / 0 warnings.

## Findings

### F-W6-E8-1 [Class C, Severity 🟢 LOW] "Metadata / From TOML" only shows Icon + Name — sparse

Real SEP-1 TOML fields (description, conditions, contact, organization, validators, holders) are not rendered. If TOML payload contains them, they're silently dropped. Currently shows: `Icon: U` + `Name: USD Coin (Anchor)`. Spec the team intended likely had more fields.

**Cross-cite:** F-Y-3 (Wave 4 simplicity); 0074 task body for SEP-1 spec.

### F-W6-E8-2 [Class B, Severity 🟢 LOW] Holder count "2" not linkable to per-asset holders list

Summary says "Holders: 2" as plain number. Click → no navigation. Per task 0074 there is no holders sub-page; if intentional, document. Otherwise spawn route.

## Cross-entity exercises

Issuer `GAFFFRANK…XXXX` → `/accounts/G…` ✓.
Tx hash → `/transactions/<hash>` ✓.

## Network requests

`/v1/assets/6` + (potentially) sub-section /transactions on asset detail (or filtered tx endpoint). Same as account: would benefit from `enabled: !!parentData` for sub-section if invalid id given.
