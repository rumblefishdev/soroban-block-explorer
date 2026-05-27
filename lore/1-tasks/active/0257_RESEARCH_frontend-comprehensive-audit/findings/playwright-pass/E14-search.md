# E14 — `/search` — Wave 6 Playwright re-pass

H1: `"Search"`. Sub-prose: "Refine your query to find transactions, accounts, contracts, tokens, NFTs, and liquidity pools."
Search input, then 6 tabs: Transactions / Accounts / Contract / Token / NFT / Liquidity Pool. Tab badge counts shown.

## Console: 0 errors / 0 warnings.

## Positive verifications — directRouteFor + L-strkey + NFT search CONFIRMED

| Test case | Expected (per post-Gate-B baseline) | Observed | Verdict |
|---|---|---|---|
| `?q=LD5MMO…O6TL` (full pool strkey) | redirect to `/liquidity-pools/L…` | URL changes to `/liquidity-pools/LD5MMO…O6TL` | ✅ F-L-1 fix landed |
| `?q=1020` (bare digit) | redirect to `/ledgers/1020` | URL changes to `/ledgers/1020` | ✅ directRouteFor.ts confirmed |
| `?q=7b9bacc8…2089` (full tx hash) | redirect to `/transactions/<hash>` | URL changes to `/transactions/7b9bacc…2089` | ✅ |
| `?q=Cat` (NFT name partial) | renders search results, NFT tab has hits, hit click → composite path | "NFT 2" badge; rendered `<a href="/nfts/CSTELLARCATSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX/2">` | ✅ NFT search → composite path |
| `?q=` (empty) | renders generic empty-state prose | "Type to search transactions, accounts, contracts, tokens, NFTs, and liquidity pools." | ✓ (different copy than no-results — see F-W6-E14-1) |

## Findings

### F-W6-E14-1 [Class C, Severity 🟢 LOW] Empty-state hint at `?q=` does NOT enumerate prefix examples; only the "no-results-after-search" hint does

`web/src/search/SearchResultsView.tsx`:
- line 109 (empty `effectiveQuery`): `'Type to search transactions, accounts, contracts, tokens, NFTs, and liquidity pools.'`
- line 99 (no-results-found): `'Try a full transaction hash, account address (G…), contract address (C…), liquidity pool (L…), or token code.'`

User arriving at `/search` from a cold link sees the generic "Type to search…" prose, never sees the `G…/C…/L…` enumeration unless they first type something that returns 0 results. F-K-4's stated fix only patched the no-results message. Partial fix.

**Cross-cite:** F-K-4 (Wave 1), F-W6-E0-4.

### F-W6-E14-2 [Class C, Severity 🟢 LOW] Search input has TWO clear-button affordances (X icon + KeyboardReturn icon)

HTML shows both a Clear button (`aria-label="Clear search"`) and a Return-key indicator. The Return icon is decorative ("hit enter to submit") but visually similar to a button. Slightly confusing.

### F-W6-E14-3 [Class A, Severity 🟢 LOW] First Tab from page-load lands on header search input (not main hero search on home)

Keyboard nav from `/` → press Tab → focus goes to the header search box, not the bigger hero search. Both reachable in 1-2 tabs respectively; not broken, but the duplication makes tab-order longer than necessary on home.

**Cross-cite:** F-W6-E1-2 (duplicate search inputs).

## Cross-entity exercises

NFT row click → composite path ✓.
Tx hash full → tx detail ✓.
Pool strkey full → pool detail ✓.
Bare digit → ledger detail ✓.
Account address full → presumably /accounts/G… (not exercised but symmetric to other cases).
