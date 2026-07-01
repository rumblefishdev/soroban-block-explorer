# E10 — `/nfts` — Wave 6 Playwright re-pass

H1: `"NFTs"`. Subtitle: "Soroban-based NFT contracts on the Stellar network".

Four filter slots above table (all render as `​` / zero-width — same finding pattern as E7).
Table 4 columns (NFT / Collection / Contract ID / Owner). 5 rows visible. Cards on left = `<img>` token icon placeholders.

## Console: 0 errors / 0 warnings.

## Findings

### F-W6-E10-1 [Class C, Severity 🟡 MEDIUM] FOUR filter slots above NFTs table — twice as many as Assets, all unlabeled

Snapshot text shows 4 filters, none with visible label/placeholder. Per F-W6-E7-1 the assets list has 2 unlabeled. Pattern of "filters without semantic context" cross-cite. Spawn or bundle in Phase 3 `XXXX_REFACTOR_filter-bar-a11y-and-labels`.

### F-W6-E10-2 [Class C, Severity 🟢 LOW] Each NFT row shows `#2 / #1 / #3 …` token IDs but only via inline text — no icon distinction

Visible: "Cat #2", "Cat #1", "Punk #3", etc. Names + token IDs intermixed; OK readability but could benefit from typographic separation (e.g. token id in chip).

## Cross-entity exercises

NFT name link → `/nfts/<contractId>/<tokenId>` composite ✓.
Contract ID `CSTELL…XXXX` truncated and shown without link — Spot-check: should it link to `/contracts/<C>`? If NFTs collections are tracked as contracts in the index, yes. Currently plain text.
Owner `GACC…XXXX` → /accounts/G… ✓.

### F-W6-E10-3 [Class B, Severity 🟡 MEDIUM] NFT row Contract ID is plain text, not a link to `/contracts/:id`

`Stellar Cats CSTELL…XXXX` plain text. Same NFTs are issued by Soroban contracts that have invocation history pages. User cannot one-click to the issuing contract.

**Cross-cite:** K-cross-entity-links (Wave 3).
