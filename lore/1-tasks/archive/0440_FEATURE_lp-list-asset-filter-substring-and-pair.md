---
id: '0440'
title: 'FEATURE: LP list asset filter — substring + pair syntax + native XLM reachable (explicitly not user regex)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0371']
tags:
  [
    backend,
    api,
    frontend,
    liquidity-pools,
    search,
    priority-medium,
    effort-small,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/366'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment ("wished someone
      implemented regex for search in pools"). Investigation found the filter is
      weaker than the reporter assumed — exact match, not substring — and that
      the input placeholder overpromises. Scoped to substring + pair syntax;
      user-supplied regex deliberately rejected (see Rejected below).
  - date: '2026-08-07'
    status: active
    who: karolkow
    note: >
      Promoted to active. Corrected the Rejected section: the original
      "unbounded backtracking" rationale is factually wrong — ClickHouse
      `match()` runs on RE2, which is linear-time and does not backtrack
      (verified on production: `(a+)+$` against a 72.7k-row scan returns in
      ~0.1s wall including client start). Decline stands on the real grounds.
      Table measured: 72 700 rows / 52 376 pools, both code columns 653 KiB
      total — a substring scan is not a cost question here.
  - date: '2026-08-07'
    status: active
    who: karolkow
    note: >
      Substring filter implemented (`positionCaseInsensitive`), pair syntax
      deferred to a follow-up. Found and fixed a second defect while measuring:
      native XLM legs store an empty code, so "XLM" returned 3 716 look-alike
      credit pools and none of the 11 687 pools actually holding native XLM.
      Predicate now matches what the row displays. Two CH-backed tests added,
      both mutation-checked. Docs + API types updated.
  - date: '2026-08-07'
    status: active
    who: karolkow
    note: >
      Pair syntax pulled back into scope and shipped — `splitn(2, '/')` at the
      handler, one AND-ed substring predicate per needle, so order-insensitivity
      needs no knowledge of canonical leg ordering anywhere. `XLM/USDC` → 215
      pools on production. Third CH-backed test (order-insensitive + both needles
      binding), mutation-checked, plus five handler unit tests for the split.
      All acceptance criteria now met.
  - date: '2026-08-07'
    status: completed
    who: karolkow
    note: >
      Pair semantics corrected before archiving: each needle now claims its own
      leg instead of being asked independently whether it matches somewhere.
      `USD/USDC` had been returning the same 2 912 pools as `USDC` alone, and
      `XLM/USDC` was letting through 18 pools whose only match was a single token
      coded `XLMUSDC`. Now 193 and 197. Four mutation-checked CH tests, ten
      handler unit tests, 223 api + 224 web tests green, clippy clean, docs and
      API types current. Implementation done and pushed as PR #382; NOT yet
      deployed, so issue #366 stays open and the fix is unproven on the real
      request path until then. One follow-up recorded under Future Work
      (identity-based asset filter) — still to be spawned on `develop`.
  - date: '2026-08-07'
    status: completed
    who: karolkow
    note: >
      DEPLOYED and verified on the real request path — the gap the entry above
      flagged is closed. `Compute` at 12:21 UTC: three Lambdas replaced (sizes
      changed, so it is genuinely new code), indexer resumed at a 7s lag with an
      empty processor DLQ, SPA synced and confirmed armed (the shipped bundle
      carries the Turnstile site key, so the Nx-cache trap did not bite).
      Live numbers match the pre-deploy measurements exactly: the reporter's own
      `xlm/kale` returns 7 pools with the native XLM/KALE one first, and
      `USDC/USDC` returns 72 rather than the 2 912 the pre-fix semantics gave.
      The follow-up became task 0466 (strkey forms rendered as accounts), which
      also carries the identity-filter direction. #366 closes on this.
---

# FEATURE: LP list asset filter — substring + pair syntax

## Summary

The liquidity-pool list filter matches a whole asset code exactly, so `USD`
returns nothing for `USDC` pools, and there is no way to filter by a _pair_
despite the input inviting it. Add substring matching and a `A/B` pair syntax.

## Current behaviour

`crates/api/src/liquidity_pools/queries.rs:975-979`:

```
AND (upper(lp.asset_a_code) = ? OR upper(lp.asset_b_code) = ?)
```

Exact equality on an upper-cased code, one code per request. Consequences:

- `USD` does not match `USDC` — the input has no partial matching at all.
- Only one leg can be constrained; there is no pair filter.
- `web/src/pages/liquidity-pools/PoolsFilterBar.tsx:65` labels the field
  **"Filter by asset pair…"**, which the backend cannot honour. The placeholder
  is the immediate user-visible defect even if the query is left alone.

Distinct from the global search bar (task 0271, completed) — that is a separate
endpoint and does not back this filter.

## Scope

All three original items shipped, plus a fourth found on the way.

1. Substring match on either leg's code — shipped.
2. Pair syntax `USDC/XLM`, order-insensitive — shipped. Initially deferred, then
   pulled back in: once the predicate was already a substring match with the
   native alias, the pair reduced to AND-ing a second needle.
3. Fix the placeholder to describe what the field actually does — shipped.
4. **Native XLM must be findable** — found during this work, shipped with it.

### The native-XLM defect (found 2026-08-07)

Native legs are stored with `asset_type = 0` and an **empty** code, while every
surface renders them as `XLM`. Substring matching alone therefore does not fix
the filter for the network's most-held asset — it makes it confidently wrong:

```
pools that actually hold native XLM                  11 687
pools a text search for "XLM" returned               3 716   ← none of the above
```

Those 3 716 are credit assets someone minted under the code `XLM`. Codes are not
unique on Stellar, and there are real `XLM/XLM` pools plus `''/XLM` pools pairing
native XLM with a look-alike. So the user typing the obvious thing got a
plausible page of the wrong pools, with no signal anything was missing.

Fix: the predicate matches what the row _displays_ —
`if(asset_type = 0, 'XLM', code)`. `XLM` now reaches 14 935 pools — the 11 687
native ones plus the look-alikes, less their overlap. `USD` is unchanged at 4 542.

**Known limit, accepted for this iteration:** the result still mixes real native
XLM with look-alike credit assets, because this filter matches _codes_, not asset
identity. Separating them needs picking a specific `(code, issuer)`, which the
per-leg params already express for API consumers but the UI does not offer.

**No minimum-length guard.** The original scope wanted 2–3 chars "to keep the
scan bounded"; the measurement below shows there is no scan to bound. A 1-char
needle just returns a wide, still-paginated result set, which is a legitimate
thing to type. A guard that silently drops the filter would instead return
_everything_ — strictly worse than the wide match it was meant to prevent.

**Measured, production, 2026-08-07** — `liquidity_pools` holds 72 700 rows /
52 376 pools; `asset_a_code` + `asset_b_code` are 653 KiB of data in total.
`USD` as substring reaches 4 542 pools where exact match reached 158. Today's
`upper(col) = ?` already defeats the index, so substring does not change the
plan class — both are the same bounded full scan over a small table.

### Pair syntax

`USDC/XLM` splits into two needles, and the query assigns each one its own leg,
accepting both assignments. Order-insensitivity falls out for free — nobody has
to know Stellar's canonical leg ordering, on either side of the wire. (Checked
anyway: the stored order is canonical, 0 violations of type → code → issuer
across 72 598 rows. Relying on it would still have been a worse design.)

The split is `splitn(2, '/')`, deliberately bounded. The field is unbounded free
text, so an unbounded split lets one request become thousands of needles, each a
pass over the table. Two is what a pair means. A third code lands _inside_ the
second needle (`XLM/BTC`), which no asset code can equal, so the query returns
nothing — correct, since a pool has two legs, and honest, since nothing was
quietly discarded.

**Each needle claims its own leg.** The first cut asked each needle
independently whether it matched _somewhere_, which is wrong the moment the two
needles overlap — one asset then satisfies both halves of the query:

| Query       | needles asked independently | needle per leg |
| ----------- | --------------------------- | -------------- |
| `USDC/USDC` | 2 912                       | **72**         |
| `USD/USDC`  | 2 912                       | **193**        |
| `XLM/USDC`  | 215                         | **197**        |

Row two is the clearest failure: `USD` is a substring of `USDC`, so a single
USDC leg satisfied both needles and `USD/USDC` returned exactly what `USDC`
returns — the pair narrowed nothing. Row three is not rounding either; the 18
pools that drop out are pools holding one token literally coded `XLMUSDC` /
`USDCXLM` beside something unrelated (`VLCC/XLMUSDC`, `GBPJPY/USDCXLM`). They
contain neither XLM nor USDC, and the old predicate let them through.

So a pair is `(a~x AND b~y) OR (a~y AND b~x)` — order-insensitive and
assignment-correct. Four binds instead of two, one extra column pass in the
worst case, unmeasurable on this table.

**Measured, production:** `XLM/USDC` → 197 pools, against 14 935 for `XLM` alone
and 2 912 for `USDC` alone. A pair with an unmatchable second code → 0.

**Three codes → empty, by construction.** `USDC/XLM/BTC` splits into `USDC` and
the literal `XLM/BTC`. No asset code contains `/`, so the second needle matches
nothing and the query returns nothing. That is the honest answer — a pool has
two legs, so there is no result to give — and it costs no validation code. The
alternative, rejecting the input with a 400, would tell the user _why_ they got
nothing; deliberately not done, because the empty state on a filter is already
the normal way to say "no matches", and a 400 on a debounced free-text field
fires mid-typing.

**Duplicate rows are already handled upstream, and are moot here anyway.** The
`page` CTE reads `FROM liquidity_pools lp FINAL`, so the duplicate rows the
unmerged ReplacingMergeTree carries collapse before the filter applies — no pool
can appear twice in a page. And they could not have changed a result regardless:
a pool's legs never change, so its duplicates never disagree about them.
Verified on production — **0** pool ids have more than one distinct leg tuple
across their rows. (Note that raw row counts quoted elsewhere in this task are
pre-`FINAL`: 72 700 rows for 52 376 pools. Pool counts here all come from
`uniqExact(pool_id)`.)

**Placeholder now closer to Figma than the interim wording.** Figma says "Filter
by asset pair…"; the field reads "Filter by asset or pair, e.g. USDC/XLM",
because it takes both a single code and a pair, and the example is the part
users cannot guess. Still a deviation — flag it in the PR.

## Rejected: user-supplied regex

The request's title asked for regex. The report itself — a screenshot — showed
`xlm/kale` typed into the filter and answered with "No pools match your filters".
Regex was the reporter's guess at a remedy, not the complaint. The complaint was
that a pair query returned nothing, and that is fixed.

**It would not have helped.** `xlm/kale` failed for two reasons: whole-code
comparison, and native XLM being stored without a code. No pattern matches an
empty column, so a regex field would still have missed the native XLM/KALE pool.

**It cannot express the gap that remains.** What separates the real USDC from the
56 other assets sharing that code is the _issuer_ — a column this filter never
touches. Pattern matching over codes cannot reach it; picking a specific asset
can. That is the direction, not a sharper pattern.

Nobody has since asked for a query that substring plus `A/B` cannot express. If
someone does, that is the evidence to reopen this — and the case to look at
first is anchoring (`^USD` excludes the ~1 900 pools where `USD` sits mid-code)
and exact match, which this task removed and did not replace.

### Two rationales that were written here and do not survive measurement

Recorded so they are not repeated, not because they are load-bearing.

- ~~"Unbounded backtracking risk"~~ — false. ClickHouse `match()` compiles to
  RE2, which is linear and does not backtrack. Verified against this table:
  `match(…, '(a+)+$')` over the full scan returns in ~0.1 s wall.
- ~~"The RE2 dialect is not the one users mean"~~ — overstated. Anchors, `.*`,
  alternation and character classes all work; only lookaround and backreferences
  error out, and nobody reaches for those against a 12-character asset code.

One operational fact does hold, if regex is ever revisited: an uncompilable
pattern surfaces as a ClickHouse exception, which `handlers.rs` maps to a 500.
The field debounces, so `USD(` is a normal keystroke on the way to `USD(C|T)` —
a regex mode would need pattern validation before the query, or it would answer
ordinary typing with server errors.

Cost is not a reason in either direction. The asset filter full-scans a 72.7k-row
table before and after this change — see the measurement in Scope.

## Acceptance criteria

- [x] Substring match on `asset_a_code` / `asset_b_code` — `positionCaseInsensitive`
      in `queries.rs`. Min-length guard dropped on purpose (see Scope).
- [x] `A/B` pair syntax, order-insensitive — `splitn(2, '/')` in the handler,
      one leg assigned per needle in the query (both orders OR-ed)
- [x] Native XLM reachable by typing `XLM` — `if(asset_type = 0, 'XLM', code)`
- [x] Placeholder text matches actual behaviour — deviates from Figma, on purpose
- [x] Query cost measured on production; substring keeps the same plan class as
      the exact-match predicate (both full-scan a 72.7k-row table)
- [x] Regex explicitly not accepted — `position` matches the needle literally, so
      `%`, `_`, `.*` are ordinary characters. Verified on production: each of them
      returns 0 pools rather than acting as a wildcard. Nothing to reject, because
      nothing is interpreted.
- [x] **Docs updated** — `docs/architecture/frontend/frontend-overview.md` LP-list
      filter line now says substring
- [x] **API types regenerated** — `openapi.json` + `generated/types.gen.ts`
      (doc-comment change only; no parameter shape change)

## Verification

- `cargo test -p api --lib decode_smoke` against a local ClickHouse carrying the
  real schema (`docker compose up clickhouse db-clickhouse-init`) with three
  seeded pools. New test `asset_code_filter_matches_substring` asserts `USD`
  returns pools and that every returned pool actually carries `USD` in a leg.
  A second test, `asset_code_filter_finds_native_xlm`, asserts `XLM` returns at
  least one pool with a native leg. The seed deliberately includes both a native
  pool and a credit asset coded `XLM`, so the test can tell them apart.
  A third, `asset_code_filter_pair_is_order_insensitive`, runs `XLM/USDC` and
  `USDC/XLM` and requires identical pool ids, requires an unmatchable second
  needle to return nothing, and requires `USDC/USDC` to return only pools with
  USDC on both legs. The seed carries a USDC/USDC pool and a native/USDC pool so
  that last assertion can tell the two semantics apart.
- Mutation-checked, every predicate: restoring `upper(col) = ?` turns the
  substring test red ("`USD` matched no pool"); dropping the native alias turns
  the XLM test red ("returned 1 pool(s) but none holds native XLM"); binding only
  the first needle turns the pair test red; and reverting the pair to two
  independent needles turns it red on the `USDC/USDC` assertion. None passes
  vacuously. All skip cleanly when `CH_URL` is unset, so CI is unaffected.
- Handler-side splitting covered by plain unit tests (no CH): pair, spaces around
  the slash, half-typed `USDC/` and `/XLM`, and 5 000 slashes staying bounded.
- Production data (read-only, `chq`): substring `USD` → 4 542 pools vs 158 for
  exact match; lowercase `usdc` → 2 912 (case-insensitivity holds); `%` → 0 and
  `.*` → 0 (metacharacters are literal); `XLM` → 14 935 vs 3 716 before, with
  11 687 native pools now reachable; `XLM/USDC` → 197; `USDC/USDC` → 72.
- Placeholder confirmed rendered in the running dev server.
- Full suites green: `cargo test -p api --lib` 223 passed, `nx run web:test`
  224 passed, `cargo clippy -p api` clean.

**Not verified locally:** the deployed request path end to end. The API runs as a
Lambda and production ClickHouse is behind mTLS, so a local process cannot reach
it — the dev server's proxy still hits the _deployed_ backend, i.e. the old
exact-match query. First real end-to-end proof is the deploy.

## Implementation notes

Five files, one behavioural change each, no new modules:

| File                                               | Change                                                                                  |
| -------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `crates/api/src/liquidity_pools/queries.rs`        | the predicate, `asset_codes: Vec<String>` on the resolved params, three CH-backed tests |
| `crates/api/src/liquidity_pools/handlers.rs`       | `normalize_asset_code` → `normalize_asset_codes` (splits the pair), five unit tests     |
| `crates/api/src/liquidity_pools/dto.rs`            | endpoint doc — feeds `openapi.json`                                                     |
| `web/src/pages/liquidity-pools/PoolsFilterBar.tsx` | placeholder + aria-label                                                                |
| `docs/architecture/frontend/frontend-overview.md`  | LP-list filter semantics                                                                |

No new query parameter, so the OpenAPI diff is documentation only.

## Design decisions

### From plan

1. **Substring over exact match.** The reported defect, and the whole reason the
   task existed.
2. **Pair via `A/B` in the existing parameter**, not a new one. The field is
   already free text and the FE already sends it; a second parameter would have
   been a public API surface for something the user types into one box.

### Emerged

3. **`position`, not `LIKE`.** Both express substring. `LIKE` would make `%` and
   `_` caller-controlled wildcards and hand us an escaping obligation on
   free text; `position` treats the needle literally, so there is nothing to
   escape and nothing to get wrong. Verified rather than assumed: `%` and `.*`
   both return 0 pools on production.
4. **Native alias in the predicate rather than at ingest.** Backfilling `XLM`
   into the stored code would fix the filter and break every consumer that reads
   the column as "credit code, empty when native" — the SAC joins in this same
   query included. The alias is local to the one place that needs it.
5. **No minimum-length guard**, against the original scope. See Scope: the guard
   was there to bound a scan that does not need bounding, and its failure mode
   (drop the filter → return everything) is worse than the wide match it
   prevents.
6. **`splitn(2)` rather than `split`.** Bounds the needle count on an unbounded
   free-text field. Chosen over a validation error for a third code because the
   empty result is already the correct answer.
7. **Needle-per-leg rather than needle-anywhere** for pairs. Caught by asking
   what `USDC/USDC` should do; the measurement then showed `USD/USDC` returning
   the same 2 912 pools as `USDC` alone, i.e. the pair narrowed nothing.
8. **Placeholder deviates from Figma.** "Filter by asset or pair, e.g. USDC/XLM"
   — the example is the part users cannot guess, and the field genuinely takes
   both shapes.

## Issues encountered

- **The task's own rejection rationale was wrong.** It claimed user regex risks
  unbounded backtracking; ClickHouse runs RE2, which is linear. Corrected in
  place before it could be repeated to the reporter. Lesson: a rationale written
  from intuition survives in a task file until someone measures it.
- **Filtering a column the UI does not display.** The native-XLM defect was
  invisible from the code — the query looked correct, and only counting pools
  against what the page renders exposed it. Nothing in the type system connects
  "stored empty code" to "displayed as XLM".
- **No local end-to-end path.** Lambda plus mTLS ClickHouse means the deployed
  request path cannot be exercised from a laptop. Worked around by running the
  real query functions against a local ClickHouse carrying the real schema
  (`docker compose up clickhouse db-clickhouse-init`) — enough to prove the SQL
  is valid and the semantics hold, not enough to prove the deployed wiring.

## Future work

- **Filter by asset identity, not by code.** Codes are not unique on Stellar:
  `XLM` still returns real native pools mixed with look-alike credit tokens, and
  the same is true of every popular code. The fix is picking a specific
  `(code, issuer)` from a list — the `GET /v1/assets?filter[code]=` endpoint
  already backs exactly that lookup, and `home_domain` (task 0450) is what
  distinguishes the impostors on screen. Needs its own backlog task; **spawn it
  on `develop`**, not on this branch.
- **`filter[asset_code]` semantics are now three things in one box** (substring,
  pair, native alias). If a fourth arrives, split the parameter rather than
  growing the mini-language.
