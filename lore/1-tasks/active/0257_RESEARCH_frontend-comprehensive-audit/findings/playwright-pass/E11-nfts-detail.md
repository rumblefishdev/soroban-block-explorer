# E11 — `/nfts/:contractId/:tokenId` — Wave 6 Playwright re-pass

H1: `"Cat #2"`. Subtitle: `"Collection: Stellar Cats"`.
Sections (rendered, but as styled-text not real `<h2>`/`<h3>`): Details, Traits, Transfer history.

## Console: 0 errors / 0 warnings.

## Positive verification — composite path navigation CONFIRMED

`/nfts/CSTELLARCATSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX/2` renders correctly without 404. Post-0264 + post-0270 fix verified end-to-end (route → render → cross-link back to NFT list).

## Findings

### F-W6-E11-1 [Class C, Severity 🟡 MEDIUM] NFT detail has h1 but NO h2/h3 elements at all

evaluated `[...document.querySelectorAll('h2,h3')]` → empty. Section headings (Details / Traits / Transfer history) are rendered as styled `<div>` or `<Typography>` without `component=` prop. Screen-reader users can't jump between sections via heading shortcut.

**Cross-cite:** F-CH a11y batch.

### F-W6-E11-2 [Class C, Severity 🟢 LOW] "Metadata unavailable" empty state for Traits — no actionable guidance

If contract didn't expose traits metadata, "Metadata unavailable" alone tells user nothing. Could add: "This NFT contract does not expose trait metadata in its WASM interface" or similar.

### F-W6-E11-3 [Class B, Severity 🟡 MEDIUM] Contract ID `CSTELL…XXXX` in Details section is plain text, not a link

Same pattern as F-W6-E10-3. User cannot click the issuing contract from the NFT detail to navigate to `/contracts/<C>`.

## Cross-entity exercises

Breadcrumb `NFTs / Stellar Cats / #2` — works (NFTs link), collection text plain.
Current owner `GACC…XXXX` → /accounts/G… ✓.
"Minted at ledger 1,015" — likely should link `/ledgers/1015`; spot-check needed.
Transfer history row "From — / To GACC…XXXX / Transaction 69ba0a…2699" — tx hash link ✓, To-account link ✓.
