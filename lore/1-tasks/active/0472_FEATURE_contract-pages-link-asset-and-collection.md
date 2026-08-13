---
id: '0472'
title: 'FEATURE: contract pages link + name what they represent (fungible/NFT links, SAC polish from /ux-expert)'
type: FEATURE
status: active
related_adr: ['0051']
related_tasks: ['0441']
tags: [frontend, contracts, assets, nfts, priority-low, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/368'
history:
  - date: '2026-08-11'
    status: active
    who: karolkow
    note: >
      Activated. 0441 is deployed and verified on production (list chips
      `SAC · CODE` with `/assets/CODE-ISSUER` hrefs, detail row, native
      `XLM` → `/assets/native`, non-SAC clean, no console errors). The
      production pass surfaced two more items, both added to scope: the
      SAC summary row packs asset + issuer into ONE cell so it reads as
      two unlabelled buttons (scope 7), and `/assets/native` titles itself
      "Asset" with a `?` avatar (scope 8) — newly visible because the SAC
      link now leads there.
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Scope extended after the /ux-expert pass over the shipped 0441 UI:
      three accepted findings added (detail header names the asset, summary
      row label "Asset", SAC chip replaces the redundant Token chip + the
      list filter label "Token" → "SAC"). The chip-vs-row question for the
      list is DECIDED (rows only, Type chips stay unlinked) — AC updated.
      Measured basis for the dedup: contract_type × is_sac cross-tab on
      prod shows Token ⟺ is_sac exactly (3,946/3,946; zero non-SAC type-0),
      so the double chip carries zero extra information.
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0441 review: the SAC chip now links its mirrored asset,
      which leaves the OTHER contract classes as the odd ones out. Measured
      on prod: every one of the 4,340 Fungible contracts has an assets row
      keyed by its own contract surrogate, and the /assets/{C…} deep-link
      already resolves it — the link is frontend-only. NFT collections are
      reachable via the existing /nfts filter[contract_id]. Zero type-0
      non-SAC contracts exist, and classic assets have no contract page, so
      SAC + these two classes close the matrix completely.
---

# FEATURE: contract pages link the asset / collection they represent

## Summary

Task 0441 made a SAC contract link the classic asset it mirrors. The same
"this contract has a face elsewhere in the explorer" relation exists for the
other two contract classes and is still unlinked:

- a **Fungible** (SEP-41) contract IS an asset — `assets` carries an
  `asset_type = 3` row keyed by the contract's own surrogate
  (4,340 of 4,340 on prod, 2026-08-10), and `/assets/{C…}` already resolves
  the contract StrKey to that asset;
- an **NFT** contract is a collection — the NFTs list already filters by
  `filter[contract_id]`.

Both are frontend-only links; no API change, no new query.

## Non-goals

- Type-0 non-SAC contracts: zero exist on prod (every type-0 is a SAC).
- Classic assets: no contract page exists to link from; the SAC case is 0441.

## Scope

Cross-links (original scope):

1. Contract detail (Fungible): an "Asset" summary row linking to
   `routes.asset(contract_id)` — same row shape as 0441's mirrored-asset row.
2. Contract detail (NFT): a "Collection" link to the NFTs list filtered by
   this contract. **The frontend URL is `/nfts?contract={C…}`**, not
   `filter[contract_id]` — `NftsListPage` registers the filter under the key
   `contract` (`useCursorPagination({ filterKeys: ['collection', 'contract'] })`)
   and maps it to the `filter[contract_id]` API param itself, after an
   `isContractId` guard. No `routes` helper builds that URL yet.
3. Contracts list: **decided** (/ux-expert, 2026-08-10) — Type chips stay
   unlinked. A linked chip points at a DIFFERENT entity (the 0441
   `SAC · CODE` case); "Fungible"/"NFT" are category labels, and a link from
   a category label to the row's own asset would read as a filter, not
   navigation. Links live on the detail rows only.

SAC polish (accepted /ux-expert findings on the shipped 0441 UI):

4. Detail header: the `Stellar Asset Contract` chip becomes
   `Stellar Asset Contract · CODE`, linked like the list chip — the page's
   landing moment should name the asset, not just flag SAC-ness
   (frontend-overview: "SAC identification must be visually clear").
5. Summary row label: "Mirrors asset" → **"Asset"** — one plain word instead
   of invented jargon (update frontend-overview wording too).
6. Chip dedup + filter relabel (both halves accepted):
   - a SAC row shows ONLY the linked `SAC · CODE` chip — the `Token` type
     chip is dropped for `is_sac` rows (prod cross-tab: Token ⟺ SAC exactly,
     3,946/3,946, so the pair is 100% redundant);
   - the list filter label `Token` → `SAC` (UI label only; the API
     `filter[type]=token` param and `contract_type` values are unchanged).
     Add `aria-label`/tooltip with the issuer on the linked chip while there —
     the bare code is ambiguous (many issuers of "USDC" on prod).

Found on the post-deploy production pass (2026-08-11):

7. **The SAC summary row packs two links into one cell.** It renders
   `Mirrors asset | POYE GCBP…FJTB` — code and issuer side by side, neither
   labelled, so they read as two anonymous buttons. The row directly above
   already does it right: `SummaryRow` takes an ARRAY of cells and renders
   `Deployer | GBJW…THEN` next to `Deployed at ledger | 54,571,433`. Split
   the SAC row the same way — `Asset | POYE` + `Issuer | GCBP…FJTB` — which
   also delivers scope 5 (the label becomes plain "Asset") and scope 6's
   disambiguation, as a visible labelled field rather than a hover-only
   tooltip. Native XLM drops the second cell; it has no issuer.
8. **`/assets/native` does not name itself XLM.** The page titles itself
   "Asset" with a `?` letter avatar, because native carries
   `asset_code = null` and `AssetDetailPage` falls back
   `asset_code ?? symbol ?? 'Asset'`. Pre-existing, but the SAC link from
   scope 4/7 now leads there, so it is on the path this task builds. The
   correct rule already exists in the pool code — `assetLegLabel` maps
   `asset_type_name === 'native' → 'XLM'` and hard-fails on schema drift
   rather than rendering a `?`. Reuse that rule for the asset title and its
   `AssetIcon`; check the assets LIST cell (`AssetsTable` passes raw
   `row.asset_code`) for the same gap.

## Findings from the native-naming pass (2026-08-11)

Scope 8 turned out to be one symptom of a wider inconsistency. Everything
below was measured on production data; all of it is fixed IN THIS TASK.

**9. Four separate rules encode "native is called XLM"** — on four different
input types, with four different empty-case behaviours:

| Site                                           | Input                       | On schema drift |
| ---------------------------------------------- | --------------------------- | --------------- |
| `pool-shared/helpers.ts` `assetLegLabel`       | `PoolAssetLeg`              | throws          |
| `assets/assetType.ts` `assetDisplayCode` (new) | asset row                   | returns `null`  |
| `transactions/cells.tsx`                       | operation `'native'` string | empty string    |
| `transaction-detail/shared/humanizeOp.ts`      | operation `'native'` string | —               |

Plus `AccountBalances.tsx` hard-coding `'XLM'`. The new helper collapsed
three asset-page call sites into one rule, but the project still carries
four. Unifying needs one constant + per-shape adapters (the inputs are not
interchangeable) — worth doing, low risk, no behaviour change.

**10. `asset_code` and `symbol` are NOT redundant** — they are disjoint.
Measured on prod (`assets` × `soroban_contract_metadata`, 2026-08-11):

| `asset_type`     | rows    | with `asset_code` | with `symbol` | with both |
| ---------------- | ------- | ----------------- | ------------- | --------- |
| 0 native         | 1       | 0                 | 0             | 0         |
| 1 classic credit | 339,454 | 339,454           | 0             | 0         |
| 3 soroban        | 4,342   | 0                 | 3,815         | **0**     |

`asset_code` is the classic ledger field (XDR alphanum4/12); `symbol` is the
on-chain SEP-41 `METADATA` symbol from `soroban_contract_metadata` (0297).
No row has both, so `asset_code ?? symbol` is a UNION of two populations, not
a fallback chain. Keep both fields.

**11. 527 type-3 assets carry neither code nor symbol** (4,342 − 3,815) and
render as "Asset" / "—". Whatever the cause, the display must stay honest:
the letter avatar keeps `?` rather than taking the first letter of the word
"Asset" (a confident "A" reads as a real ticker).

**11b. Two DIFFERENT causes hide behind those 527** — chain-verified with
the `stellar` CLI against mainnet RPC, 5 contracts sampled (2026-08-11):

| Contract        | `symbol()` on chain | our DB | verdict    |
| --------------- | ------------------- | ------ | ---------- |
| `CAKJ4KXW…HLRO` | no such function    | no row | honest     |
| `CCZ5NLMX…IUCL` | no such function    | no row | honest     |
| `CDIFFGCJ…BQPD` | no such function    | no row | honest     |
| `CDQLKMI4…GPXT` | **`"ALPHASH"`**     | no row | **missed** |
| `CBNMAFRH…A4MY` | **`"AVXO"`**        | no row | **missed** |

Three are genuine partial SEP-41 implementations — they expose
`balance`/`transfer` (hence the Fungible classification) but never
implement the metadata half, so there is nothing to display and `?` is the
correct, honest answer.

The other two DO publish a name and symbol, and we drop them. Root cause,
read off the chain: `ALPHASH` keeps its metadata as SEPARATE instance-storage
entries — `Vec[Symbol("Name")]`, `Vec[Symbol("Symbol")]`,
`Vec[Symbol("Decimals")]` — while `is_metadata_key`
(`crates/xdr-parser/src/token_metadata.rs:115`) matches only two shapes:
`Symbol("METADATA")` (soroban-token-sdk / SAC) and `Vec[Symbol("Metadata")]`
(OZ NFT). A third shape exists on chain and is silently skipped. Same class
of bug as the OZ-NFT shape that comment already documents.

Sanity check on the pipeline: `CBR6BXBR…BVKL`, which we DO have metadata
for, returns `symbol()` = `"VTAPI0"` — matching our row. The extractor is
right about the shapes it knows; it just does not know all of them.

Scale: 2 of 5 sampled → **order of 200 assets** (extrapolation, not a
measurement — the raw instance storage is not in ClickHouse, so the shape
cannot be counted without re-reading the chain).

**Re-decided (2026-08-11, later): split to [[0473]].** The first call was
"fix in this task — parser + re-parse", but the standard-compliance check
unravelled it: SEP-41 defines metadata as an INTERFACE (`name()` /
`symbol()` / `decimals()`), not a storage shape, so ALL THREE storage
layouts we read are per-library conventions and the layout list is
unbounded. The parser fix for the third layout was implemented and
chain-verified here, then PULLED OFF this branch and parked as
`git am`-ready patches in 0473's `patches/` — nothing ships ahead of that
task's policy decision. The drain, the negative marker and the policy all
live in 0473; this task is frontend-only again.

Task 0340 already ran this exact play for OZ NFT collection names, and its
two halves are the template here:

1. **Parser.** `is_metadata_key` learns the third shape. Note it is not just
   another key to match: the first two shapes hold ALL fields in one `Map`
   under one key, while this one spreads `Name` / `Symbol` / `Decimals`
   across THREE sibling entries — so `extract_token_metadata` has to fold
   several entries into one `TokenMetadata`, not just `find()` one. Unit
   tests use the shapes read off mainnet (`ALPHASH` below).
2. **Backfill.** A parser fix alone recovers a contract only on its NEXT
   instance write, which for a dormant token may be never. 0340 solved the
   same remainder with an RPC drain subcommand in `backfill-enrichment-runner`
   (`nft-collection-name`); the analogue here drains type-3 assets that have
   no `soroban_contract_metadata` row, calling `symbol()` / `name()` /
   `decimals()`. It doubles as the measurement: the exact split between
   "shape we missed" and "genuinely nameless" cannot be counted in ClickHouse
   (no raw instance storage is stored — `soroban_contracts` keeps 8 columns,
   none of them the storage map).

Watch out on the drain: roughly three fifths of the candidates have no such
functions at all, and the 0340-style predicate ("still missing a name") would
re-hammer RPC for them on every run. Needs a negative marker, not just
`--force-retry`.

**12. The supply unit dropped native.** `AssetSummary` and the `AssetsTable`
supply column both computed the ticker as `asset_code ?? symbol`, so native
showed a bare number with no unit. Same rule, two more call sites.

**13. Pool legs do not link native.** `legHref` returns `undefined` for
`asset_type === 0`, with the rationale "Native has no on-chain address in
classic protocol". That predates task 0243 — `/assets/native` is now the
canonical token and every other surface links it. Verified on pool
`LD537AFE6YVN3UKBL43XX6OPLLEDWBSDSNETZADJRKCCLL357OIGAMSH`: "XLM" appears
twice as plain text, zero links.

**14. Searching "XLM" does not return XLM.** `search_assets` ends in
`LIMIT {per_group_limit}` with **no `ORDER BY`**, so the ten returned rows
are whichever the scan reached first. The native special-case in the WHERE
clause fires, but the row loses to `SEVXLM` / `ISHXLM` / `SFKEXLM`… Measured:
`q=XLM` → 10 hits, native absent; `q=native` → native first. There is no
relevance ranking at all — an exact match does not win. Affects every asset
search, not just native.

**15. The assets list has no `Native` type chip** (`ASSET_TYPE_FILTERS` =
All / Classic / Soroban), so the native row is reachable only by paging. The
API accepts `filter[type]=native` and returns it.

**15b. Re-decided (2026-08-11): no `Native` chip. Two other fixes instead.**
Measured while designing it (prod + API, dev proxy):

| Question                           | Answer                                                                                                          |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Does native have a SAC?            | **Yes** — `asset_sac` cross-tab: type 0 → 1 row, `sac_deployed` (contract `CAS3J7GY…OWMA`)                      |
| Is native part of `Classic`?       | **No** — distinct `asset_type` (0 vs 1); `filter[type]=classic_credit` returns only credit assets               |
| Do Soroban tokens ever have a SAC? | **No** — 0 of 4,342, and structurally impossible (a SAC wraps a classic asset; a Soroban token IS the contract) |

Three consequences:

1. **A chip that always returns exactly one row is a tab pretending to be a
   filter.** Dropped.
2. **"Show native first" is not a display tweak.** The list is a keyset walk
   over the `assets` PK (4-tuple + `id`) with a cursor — there is no
   relevance ordering to pin a row into. Pinning outside the keyset either
   duplicates the row on the page where it naturally falls or needs a
   special-cased first page the rest of the pagination does not know about.
   Backend cursor work, not frontend.
3. **The real discovery path for XLM is search — which is broken (finding
   14).** Fixing the ranking makes `q=XLM` return XLM first and serves every
   asset, not just native. Finding 14 therefore absorbs this and moves up
   the order.

What replaces 15: nothing — see 15c.

**15c. RETRACTED (2026-08-13): the impossible-combination guard already
exists.** I recorded "the UI offers `Has SAC` + `Soroban` and answers with an
empty list" after calling the API directly with both params and seeing zero
rows. The UI never sends that pair: `AssetsListPage` makes the two mutually
exclusive — clicking `Soroban` clears `sac`, clicking `Has SAC` clears the
type, each as ONE atomic URL update so last-click wins, plus a guard that
ignores a stale `sac` from a pasted deep link. Shipped in task 0339
(`960eb4a1`), long before this task.

The auto-switch is also the better design than the guard I proposed: it
answers the click instead of refusing it. Karol called this before I checked.

Method note, since this is the second time: an API-level probe says what the
API accepts, never what the UI sends. The finding needed one look at the
filter handler.

Open naming question, deliberately not folded in: the `Classic` chip filters
`classic_credit` only, but the label reads like "everything that is not
Soroban". `Classic credit` would be accurate. One word, no logic change, but
it is user-visible wording — needs a call, not a drive-by edit.

**16. Total supply wraps mid-number** on `/assets/native`:
`105,410,0 / 95,815.54 / 27811`. Caused by `overflowWrap: 'anywhere'` in
`AssetSummary` — a deliberate earlier fix (F4) to stop the longest supply in
the system from overflowing into the adjacent "Holders" cell. XLM has both
the longest value (22 chars) and a half-width cell. Needs either a full-width
supply row or breaking only at group separators.

## Acceptance criteria

- [x] Fungible contract detail links its asset page; vitest case
- [x] NFT contract detail links its filtered collection view; vitest case
- [x] /ux-expert pass on the chip-vs-row question for the list; decision
      recorded — rows only, Type chips stay unlinked (2026-08-10, see Scope 3)
- [x] Detail header chip names + links what the contract IS — for EVERY class,
      not just SAC (`contractFace`). Consistency check while building it found
      the same 0441-shaped gap one level down: the header named the class only
      for SAC, so a Fungible token and a 10k-token NFT collection both rendered
      a bare "Contract". Only SAC carries `· CODE`, because `sac_asset` is the
      one identity the contract endpoint puts on the wire — a Fungible's symbol
      and a collection's name would each cost an API change or a round-trip.
      The summary row stays SAC-only (it adds the issuer); the other classes
      would get a second link to the same target. NAMING what a Fungible or an
      NFT contract is (not just its class) needs an API field → [[0483]].
- [ ] SAC summary row split into two labelled cells (`Asset` + `Issuer`),
      native rendering the asset cell only; vitest case
- [ ] SAC rows show a single chip (`SAC · CODE`, no `Token` chip); filter
      label reads `SAC`; API params untouched
- [ ] Linked SAC chip carries an issuer tooltip / aria-label
- [x] `/assets/native` titles itself `XLM` with an XLM avatar, via one shared
      rule (`assetDisplayCode`); assets list cell + avatar fixed; vitest cases
- [x] One native→XLM constant, not five (finding 9) — `NATIVE_ASSET_CODE` +
      an `isNativeAssetString` adapter for the operation-string shape; the
      per-site EMPTY-case behaviour is deliberately left alone (throw vs null
      are two correct answers). No behaviour change
- [x] Supply unit shows `XLM` for native — detail + list columns (finding 12)
- [x] Unnamed assets keep the `?` avatar, never a fake initial (finding 11)
- [ ] Pool legs link native to `/assets/native` (finding 13); vitest case
- [ ] Asset search ranks exact matches first so `q=XLM` returns XLM
      (finding 14) — backend `ORDER BY`, API params unchanged. This is ALSO
      the answer to "how is native discoverable" (finding 15b)
- [x] `Native` type chip — decided AGAINST (finding 15b): one-row filter,
      pinning fights the keyset cursor, search is the real path
- [x] The impossible `Has SAC` + `Soroban` pair — RETRACTED (finding 15c):
      the mutual exclusion already shipped in task 0339; my finding was wrong
- [ ] `Classic` vs `Classic credit` chip label — decision recorded either way
- [x] Total supply stops breaking mid-number (finding 16) — variant A, supply
      takes a full-width row and Holders the next one
- [ ] `asset_code` / `symbol` both kept — measured disjoint, documented
      (finding 10)
- [ ] Finding 11b in full — parser third layout (parked as patches), RPC
      drain, negative marker, split measurement, compliance policy
      (deferred to 0473 — see the re-decision under finding 11b)
- [x] No API surface change at all — frontend-only, no api-types regen. The
      search ordering fix that would have broken this went to [[0482]]
- [ ] Docs: frontend-overview §6.8 + §6.10 updated

## Notes

Asset detail already links back to the contract (`AssetSummary`, deployed
contracts only), so after this task the contract ↔ asset relation is
navigable in both directions for every class that has one.
