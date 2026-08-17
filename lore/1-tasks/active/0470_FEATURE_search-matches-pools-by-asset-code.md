---
id: '0470'
title: 'FEATURE: pool search consistency — same rules in the global box and the list filters'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0440']
tags:
  [api, search, liquidity-pools, consistency, priority-medium, effort-medium]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/366']
history:
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Found by the regression sweep. `GET /v1/search?q=KALE` reports
      `Liquidity Pool 0` while the pools page returns 58 pools for the same
      needle and 7 for `xlm/kale`. Two different endpoints —
      `crates/api/src/search/` vs `crates/api/src/liquidity_pools/` — and
      only the second learned asset codes in task 0440.
      Direction set by Karol: the two must behave the same, not "pick the
      cheap option". The performance argument for the current id-only point
      seek was measured against real data and does not hold at this table
      size: the full pools-page predicate over every pool costs 47 ms /
      73 898 rows / 3.33 MiB, and today that search arm does nothing at all
      for a non-hash query.
  - date: '2026-08-14'
    status: active
    who: karolkow
    note: >
      STAGE 1 SHIPPED — merged as #409. Both surfaces now match pools with one
      shared predicate, the pools filter also takes an `L…` identifier, and a
      review round added a length gate that keeps account- and contract-shaped
      queries off the pools scan entirely. Verified against production data by
      running the generated SQL directly: `KALE` 58, `xlm/kale` 7, matching the
      pools page, and 0 of 75 218 rows change label under the unified
      native-XLM check. 533 API tests green.
      Two criteria stay open on purpose. The latency measurement on a real
      query mix was never run. And the set-membership check belongs on a local
      stack (`cargo run -p api --bin local` against prod ClickHouse) rather
      than production — deploys run weekly and independently, so nothing here
      waits on one.
      Stage 2 — the same rule on the assets and NFT lists, and folding
      `pool_id_from_text` into a shared recogniser — is untouched. Task moves
      to active because stage 2 is real remaining work, not a backlog idea.
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Scope widened and 0471 folded in on Karol's call: the two were one
      subject split in two, which cuts against the standing preference for
      larger bundled tasks. Stage 1 (pools, both directions) is IMPLEMENTED on
      `feat/0470_search-pools-by-asset-code`; stage 2 (the remaining lists and
      lifting the shape recogniser) is open. Surveying every list showed the
      same gap twice more: contracts already matches its own `contract_id` in
      `filter[q]`, assets and NFTs do not. The recogniser that decides "is this
      text an identifier" also exists twice — `search::classifier` on the
      server and `directRouteFor` on the frontend, whose own doc comment
      already states the policy this task generalises.
  - date: '2026-08-17'
    status: active
    who: karolkow
    note: >
      STAGE 2 SCOPE PINNED by a repo-wide sweep of the native-XLM
      representation. Four conventions exist, not two: the typed
      `asset_type = 0`, the empty stored code, the `NATIVE_ASSET_ID`
      surrogate (negative, `hash64("native")`), and the literal string
      `"native"` on the wire. The typed and surrogate forms are already
      clean and mutually pinned by a unit test; the whole residue is the
      empty-code form, and it lands squarely inside this task's stage 2.
      Eight items recorded below, ranked. Item 1 is the SAME defect stage 1
      fixed for pools, live on the assets list. One candidate risk from the
      sweep was refuted by measurement rather than carried: the native
      `assets` row holds the real surrogate, not the schema default, so the
      balances join is sound.
      The 0264 StrKey-only debt is folded in here as well — its follow-up
      was named in prose and never numbered, and the recogniser merge in
      this stage is the only place where "does hex stay?" gets decided.
---

# FEATURE: pool search consistency — one rule per entity, both directions

## Summary

Searching an asset code in the header search box reports zero liquidity pools
while the pools page lists dozens for the same text. The two surfaces answer
the same question differently, and the global one is wrong more often.

The requirement is **parity**, not a smaller variant of it: whatever the pools
page matches, global search matches.

## Why this matters more than it looks

Task 0440 taught the pools page to match asset codes as substrings, with `A/B`
pair syntax and a native-XLM rule — shipped 2026-08-07 and closing issue #366.
A reader who types `KALE` into the main search box now gets `Liquidity Pool 0`,
which reads as **that fix not working**. The inconsistency actively undermines
the shipped feature.

## Context — why it is id-only today

Deliberate, and documented in the code: `pool_id` is the full `ORDER BY` key,
so `pool_id = unhex(?)` is a granule-pruned point seek, and `search_pools`
fires only for a hash-shaped query (`crates/api/src/search/queries.rs:299`).
That was a sound default when nothing else matched pools.

**The cost of dropping it, measured on production:**

|                                     |                                  |
| ----------------------------------- | -------------------------------- |
| Unique pools                        | 52 472 (73 880 rows)             |
| Full-table predicate for one needle | **47 ms**, 73 898 rows, 3.33 MiB |
| Pools matched for `KALE`            | 58                               |

`/v1/search` already fans out six queries in parallel, so this lands on the arm
that currently returns nothing for non-hash input. Keep the point seek for
hash-shaped queries — it stays free — and add the code path beside it.

## Implementation

Reuse the 0440 predicate rather than writing a second one; two copies of this
rule will drift, and the native case is exactly where a re-implementation goes
wrong:

```rust
positionCaseInsensitive(if(lp.asset_{side}_type = 0, 'XLM', lp.asset_{side}_code), ?) > 0
```

- **Native XLM is stored with an empty code.** Without the `if(type = 0, …)`
  arm, `XLM` matches thousands of impostor codes and misses every real XLM
  pool — the exact bug 0440 found and fixed.
- **Pair syntax `A/B`** — each needle claims its own leg, order-insensitive.
  `normalize_asset_codes` (`liquidity_pools/handlers.rs:186`) already yields at
  most two needles; lift it rather than re-parsing.
- Extract the shared predicate so `liquidity_pools` and `search` cannot
  disagree again.

## Stage 2 — the same rule on the remaining lists (folded in from 0471)

### Not this: routing list filters through global search

Worth stating, because it is the obvious idea and it is wrong. Global search
and a list filter are different tools:

|             | `/v1/search`                          | list filter                           |
| ----------- | ------------------------------------- | ------------------------------------- |
| Purpose     | identify an entity and navigate to it | narrow a table being browsed          |
| Returns     | `SearchHit` — identifier + label      | full rows (reserves, TVL, holders, …) |
| Volume      | ≤50 per bucket, hard ceiling          | cursor pagination over the whole set  |
| Composition | one query string                      | `filter[x]` AND `filter[y]` AND sort  |
| Exact id    | redirects to the detail page          | must stay on the list                 |

A list calling search would receive identifiers with none of the columns it
renders, and lose pagination. What is genuinely duplicated is the _rules_.

### The measured gap

| List            | free-text filter     | matches                 | accepts its own id?                            |
| --------------- | -------------------- | ----------------------- | ---------------------------------------------- |
| contracts       | `filter[q]`          | `contract_id` substring | **yes**                                        |
| liquidity pools | `filter[asset_code]` | asset codes             | **stage 1**                                    |
| assets          | `filter[code]`       | code, name, symbol      | **no**                                         |
| NFTs            | `filter[name]`       | enrichment name         | **no** (separate `filter[contract_id]` exists) |

Transactions and accounts have no free-text filter — typed id filters only —
so they are out of scope until one is added.

### The policy already exists

`web/src/search/directRouteFor.ts` states it outright:

> "Adding more FE shortcuts here is rarely the right call — keep classifier
> logic on the server unless the entity type has no search bucket."

The decision that shape recognition belongs on the server, in one place, was
already taken. It never reached the list filters.

### Work

- **Recogniser to `common`.** `search::classifier::classify` is private to the
  search module. Lifted, any handler can ask "is this an identifier, and of
  what kind" with the same answer the search box gives. Stage 1 needed only the
  pool half and added `pool_id_from_text` to `common::pool_asset_codes`; that
  function must FOLD INTO the general recogniser rather than become the first
  of four copies — otherwise this task removes one duplication and creates
  another.
- **Per-entity predicates to `common`,** each called by both the list and its
  search bucket — the shape `common::pool_asset_codes` already has.
- **Settle the hex exception while the recogniser is being merged.** Task 0264
  made the SEP-23 StrKey the single accepted spelling and deferred the search
  endpoint (its phases 3, 9, 10) to a follow-up that was described in prose and
  never given a number, so nothing tracks it. `search::classifier` still takes a
  64-char hex string as a transaction or pool id. Stage 1 reached for that
  acceptance as a model, copied it onto a new filter, and had to be corrected —
  the exception read as the rule. Merging the two recognisers is the moment the
  question has to be answered rather than inherited: either hex is dropped and
  0264 closes for real, or it stays with a stated reason and a return condition.
  Recorded as debt in `backend-overview` §6.2 in the meantime.

Order: assets first (an asset id is `CODE-ISSUER` or a contract StrKey, so the
recogniser has real work to do), NFTs second (a contract StrKey, and the typed
filter already exists to reuse), contracts last — only to move its existing
inline rule into the shared module.

### Native XLM — the same defect class on the remaining surfaces

Swept across the whole repository on 2026-08-17, after stage 1 unified the pool
label. Four representations of native XLM exist:

| Convention                                       | Where it lives                                                      |
| ------------------------------------------------ | ------------------------------------------------------------------- |
| `asset_type = 0`                                 | `assets`, `liquidity_pools.asset_{a,b}_type`, parser enums          |
| empty stored code `''` + `issuer_id = 0`         | `assets.asset_code`, pool legs, `operations_appearances.asset_code` |
| surrogate `NATIVE_ASSET_ID` (`hash64("native")`) | `balances`, `operation_asset_appearances`, `lp_operation_amounts`   |
| literal `"native"`                               | parser JSON details, `/assets/:id` route token, prices-view key     |

The typed form and the surrogate are already consistent and pinned together by
`liquidity_pools/queries.rs` (`asset_id(0, "", 0, 0) == NATIVE_ASSET_ID`, with a
doc comment stating that the two conventions meet only through that equality).
Everything below is the empty-code form.

1. **`assets` list `filter[code]` cannot match native XLM.**
   `crates/api/src/assets/queries.rs` matches
   `positionCaseInsensitive(a.asset_code, ?)` with no `asset_type = 0` arm.
   Native is stored with an empty code, so `XLM` returns only the credit assets
   minted under a code containing `XLM` — a confident wrong answer, not an empty
   one. Byte-for-byte the defect stage 1 fixed for pools, on the list the
   frontend wires to `AssetsListPage`. Fix is the same one predicate; the guard
   test already exists next door in `common::pool_asset_codes`.
2. **The list-page test fixture hides item 1.** `AssetsListPage.test.tsx` builds
   native with `asset_code: 'XLM'`; the API returns `null`.
   `AssetDetailPage.test.tsx` fixed exactly this fixture and left a comment
   saying it "hid the real gap from the tests" — the sibling file still has it.
   Item 1 cannot be considered fixed while the fixture lies.
3. **`operations_appearances` carries no asset type.** The writer stores
   `asset_code.unwrap_or_default()`, so native and a parse failure are identical
   rows. The frontend fallback in `humanizeOp.ts` is correct only because a
   one-off cross-check against `operation_asset_appearances` proved it
   (11 168/11 168 and 55 582/55 582); the correctness lives in a comment rather
   than in a type, and any writer change breaks it silently. Adding
   `asset_type` — or the `asset_id` surrogate, which the neighbouring tables
   already use — retires the class.
4. **`stroops.ts` turns an absent code into `XLM`.** A shared formatter used by
   operations, pools and balances appends the native unit whenever the code is
   null or empty. Callers guard upstream today, so the default silently converts
   "asset unknown" into "XLM" — the display failure mode the project already
   ruled out elsewhere.
5. **Pool legs lose their SAC on an empty-code guard — FIXED in #417, and the
   history matters.** The `!= ''` guards in both the pool detail and the pool
   list meant a native leg reported a NULL contract id, although native has a
   deployed SAC and an `asset_sac` row.

   The guard was **deliberate**, not an oversight — `a19ac8f6` added it because
   the join _did_ surface the native SAC, and removed it to match Postgres,
   which returned NULL there. Two of its three stated reasons no longer hold:
   Postgres is retired, and "the frontend renders native legs unlinked" has
   been false since `a8028d82` linked them to `/assets/native`. The third —
   that the SAC mirror is network-dependent — does not separate native from
   anything else, since every SAC address is network-dependent and we publish
   the others.

   What settled it: `/v1/assets/native` and the assets list **already report
   that same SAC** as `sac_contract_id`, and the frontend renders it in both.
   The pool leg was the only surface withholding it, so one asset described
   itself two ways depending on which endpoint was asked.

   Guarded at module level (sabotage-verified) rather than behaviourally,
   because both pool queries are inline string literals. The follow-up is in
   the acceptance criteria.

   The icon stays NULL for native and that is correct: `asset_enrichment` holds
   no native row at all (measured: 0). The guard was never what withheld it.

6. **REFUTED — not work.** The sweep flagged `assets.id Int64 DEFAULT 0` as a
   possible break in the balances join. Measured on production: the native row
   holds the real surrogate, so the join is sound. Recorded so the same
   suspicion is not re-raised.
7. **The native regression guards do not run in CI.** The ClickHouse-backed
   smoke tests that pin stage 1's fix are gated on `CH_URL`, so the tests
   protecting the fixed bug are exactly the ones a normal run skips. Overlaps
   task 0478's gate work — check it before duplicating.
8. **`asset_type = 2` means two different things — and the sweep called this
   harmless, which was wrong.** `credit_alphanum12` in the pools module (the
   raw XDR asset type), the retired SAC facet in the assets module (our own
   enum). `0` agrees in both, so native is genuinely unaffected.

   The sweep concluded from that: two correctly-mapped columns, worst case a
   confused reader. It only ever compared the two `asset_type_name` display
   functions, and those really are both correct. What it never asked was
   whether a value **crosses** from one space into the other — and it does.

   `f4a2f2a4` (task 0489) is the answer, written the same day and
   independently: a pool leg's XDR type was passed to `ids::asset_id`, which
   reads its argument as the assets enum. A `2` meant `credit_alphanum12` on
   the way in and the retired SAC on the way out, so the function fell through
   to an id no row is stored under. The leg then matched nothing and the API
   returned it as null — **279 452 of 1 738 948 recent operations, 16.1%,
   rendered one-sided on production**, and 59% of pools carry a type-2 leg.

   Read-path only; the indexer had written both legs correctly all along.
   `ids::asset_id` was deliberately left alone, because `2` must keep meaning
   SAC for everything reading the assets enum.

   Note for this task, since it repeats a pattern already in items 1 and 2:
   the existing surrogate test used `"TF"`, a four-character code, and so
   agreed with the bug. Three defects in one sweep hidden by a fixture that
   picked the one input where the broken code is right.

   **What stands, restated:** the two display mappings are correct and need no
   change. What is NOT safe is treating `asset_type` as one value with one
   meaning across module boundaries. This stage's argument — "the typed check
   is the reliable one" — holds only within a single type space, and that
   qualifier belongs in the shared code stage 2 extracts.

Items 1 and 2 are one change and belong together at the front of stage 2, since
they are the assets list this stage already had to touch. Items 3 and 4 are a
larger cut (a schema column, a shared formatter) and may be split out once 1–2
land. Items 5, 7 and 8 are small and independent.

## Acceptance criteria

- [ ] Any query that returns pools on the pools page returns THE SAME POOLS in
      global search, up to that bucket's cap — verified on `KALE`, `xlm/kale`
      and `USDC`. Equal COUNTS are not achievable and never were: search caps
      every bucket at `MAX_LIMIT` 50 (default 10), while `KALE` matches 58
      pools. The criterion is set membership within the cap, not parity of N
- [x] Native XLM behaves identically on both surfaces (0440's rule preserved,
      not re-implemented)
- [x] Hash-shaped queries keep the point-seek path — no scan introduced for
      the case that is free today
- [x] The predicate exists in ONE place, shared by both endpoints
- [ ] Search latency measured before and after on a real query mix
- [ ] **Stage 2:** pasting an entity's identifier into its list's free-text
      filter selects that entity on every list that has such a filter
- [ ] **Stage 2:** shape recognition lives in ONE place, server-side;
      `pool_id_from_text` folded into it rather than left as a parallel path
- [ ] **Stage 2:** each entity's match predicate is shared by its list and its
      search bucket — no rule implemented twice
- [ ] **Stage 2:** existing behaviour preserved — a plain code/name still
      matches as before, and no list loses a filter it has today
- [ ] **Stage 2:** `XLM` typed into the assets list filter returns native XLM
      (sweep item 1), and the list-page fixture carries the null code the API
      actually returns (item 2) — the second is what makes the first testable
- [ ] **Stage 2:** the two pool queries are extracted from inline string
      literals so the native-SAC rule can be pinned behaviourally. #417 could
      only guard it at module level — a text check for a re-added leg-code
      guard, sabotage-verified but blind to anything subtler
- [ ] **Stage 2:** every remaining empty-code native site from the sweep is
      either fixed or carries a stated reason — items 3 and 4 (now task 0495),
      and 7 (waiting on 0478). Items 1, 2 and 5 shipped in #417; 6 was refuted
      by measurement; 8 was fixed by task 0489. Dropping any of them silently
      is not allowed
- [ ] **Stage 2:** the shared code carries the type-space qualifier item 8
      turned out to need — an `asset_type` is only comparable within the enum
      it came from, and a pool leg's XDR type is not the assets enum. This is
      the rule `f4a2f2a4` had to learn the expensive way
- [ ] **Stage 2:** the hex exception in `search::classifier` is settled, not
      inherited — either removed with 0264 closed, or kept with a written reason
      and a return condition, and `backend-overview` §6.2 updated either way
- [x] **Docs updated** — search contract under `docs/architecture/**` states
      what matches a pool, and the list-filter contract states that a
      free-text filter also accepts the entity identifier
- [x] **API types regenerated** — only if the search response shape changes
