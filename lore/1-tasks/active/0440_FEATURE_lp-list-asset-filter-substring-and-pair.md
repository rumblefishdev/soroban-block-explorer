---
id: '0440'
title: 'FEATURE: LP list asset filter — substring + pair syntax + native XLM reachable (explicitly not user regex)'
type: FEATURE
status: active
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

`USDC/XLM` splits into two needles that the query AND-s, each still "substring of
_some_ leg". Order-insensitivity falls out for free — nobody has to know
Stellar's canonical leg ordering, on either side of the wire. (Checked anyway:
the stored order is canonical, 0 violations of type → code → issuer across
72 598 rows. Relying on it would still have been a worse design.)

The split is `splitn(2, '/')`, deliberately bounded. The field is unbounded free
text, so an unbounded split lets one request become thousands of needles, each a
pass over the table. Two is what a pair means. A third code lands _inside_ the
second needle (`XLM/BTC`), which no asset code can equal, so the query returns
nothing — correct, since a pool has two legs, and honest, since nothing was
quietly discarded.

**Measured, production:** `XLM/USDC` → 215 pools, against 14 935 for `XLM` alone
and 2 912 for `USDC` alone. A pair with an unmatchable second code → 0.

**Placeholder now closer to Figma than the interim wording.** Figma says "Filter
by asset pair…"; the field reads "Filter by asset or pair, e.g. USDC/XLM",
because it takes both a single code and a pair, and the example is the part
users cannot guess. Still a deviation — flag it in the PR.

## Rejected: user-supplied regex

The original request was for regex. Still not shipping it — but **not** for the
reason first written here.

**Correction (2026-08-07).** The original rationale said an arbitrary pattern is
an "unbounded-backtracking risk". That is false. ClickHouse `match()` compiles to
RE2, which is linear in the input and does not backtrack, so the classic
catastrophic patterns are a non-event. Verified on production against this exact
table: `match(…, '(a+)+$')` over the full 72 700-row scan returns in ~0.1 s wall,
client start included. Anyone can refute the old reason in a minute, so it must
not be the one we give the reporter.

The reasons that do hold:

- **It answers a problem nobody has.** The reported symptom is that `USD` returns
  nothing for `USDC` pools — exact match, not missing regex. Substring plus
  `A/B` fixes precisely that, and covers the realistic uses (`USD…`, `…BTC`,
  `USDC/XLM`). Regex is the reporter's guess at a remedy, not the complaint.
- **The dialect is not the one users mean.** RE2 has no backreferences and no
  lookaround. A user typing `(?=.*USD)` gets an error, not a result, and the
  gap between "regex" and "RE2 regex" turns into a permanent stream of
  bug reports we would have invited.
- **It is a permanent public API contract for a filter box.** Once a caller-supplied
  pattern is a documented query parameter it cannot be narrowed later, and it drags
  in a validator: a malformed pattern otherwise surfaces as a ClickHouse exception,
  i.e. a 500 driven by free text.

Cost is not one of the reasons, in either direction. The asset filter is a full
scan today (`upper(col) = ?` cannot seek an index) and stays one after this
change — see the measurement in Scope.

Record the corrected reasoning in the reply to the reporter, not just here.

## Acceptance criteria

- [x] Substring match on `asset_a_code` / `asset_b_code` — `positionCaseInsensitive`
      in `queries.rs`. Min-length guard dropped on purpose (see Scope).
- [x] `A/B` pair syntax, order-insensitive — `splitn(2, '/')` in the handler,
      one AND-ed predicate per needle in the query
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
  `USDC/XLM` and requires identical pool ids, then requires an unmatchable
  second needle to return nothing.
- Mutation-checked, all three predicates: restoring `upper(col) = ?` turns the
  substring test red ("`USD` matched no pool"); dropping the native alias turns
  the XLM test red ("returned 1 pool(s) but none holds native XLM"); binding
  only the first needle turns the pair test red. None passes vacuously. All skip
  cleanly when `CH_URL` is unset, so CI is unaffected.
- Handler-side splitting covered by plain unit tests (no CH): pair, spaces around
  the slash, half-typed `USDC/` and `/XLM`, and 5 000 slashes staying bounded.
- Production data (read-only, `chq`): substring `USD` → 4 542 pools vs 158 for
  exact match; lowercase `usdc` → 2 912 (case-insensitivity holds); `%` → 0 and
  `.*` → 0 (metacharacters are literal); `XLM` → 14 935 vs 3 716 before, with
  11 687 native pools now reachable; `XLM/USDC` → 215.
- Placeholder confirmed rendered in the running dev server.
- Full suites green: `cargo test -p api --lib` 223 passed, `nx run web:test`
  224 passed, `cargo clippy -p api` clean.

**Not verified locally:** the deployed request path end to end. The API runs as a
Lambda and production ClickHouse is behind mTLS, so a local process cannot reach
it — the dev server's proxy still hits the _deployed_ backend, i.e. the old
exact-match query. First real end-to-end proof is the deploy.
