---
id: '0494'
title: 'REFACTOR: frontend consistency sweep — one chip size, one placeholder, one name per thing, real heading hierarchy'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0366', '0483', '0472']
tags: [frontend, ui, consistency, priority-low, effort-small]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Bundled from an audit run after the 2026-08-17 deploy, which started
      from one reported symptom (chips on two sibling detail pages look
      unrelated) and swept the frontend for the same class of drift. Each
      item is individually a few lines; filed as one task because they share
      a single review and a single PR, and because six one-line tasks would
      be worse than one. The measurements below are from the code on
      `develop` and computed styles on production.
---

# REFACTOR: the same thing should look and be called the same everywhere

## Why one task

Every item here is small enough that its own task would cost more to track
than to fix. They also share a shape: some surface drifted from the
convention the rest of the app follows, and nothing catches it because none
of them is wrong on its own page — only against the others.

Ordered by what a user would notice first.

## 1. The contract detail chip is the only `md` chip in the app

Measured on production, comparing `/contracts/CB23…OUOV` with the asset it
links to, `/assets/KALE-GBDV…KALE`:

|                 | asset detail | contract detail |
| --------------- | ------------ | --------------- |
| font-size       | 12px         | 14px            |
| font-weight     | 700          | 500             |
| padding         | 2px 8px      | 4px 12px        |
| rendered height | 21px         | 28px            |

Same family (Satoshi); the "different typeface" impression is the weight
swap on a chip a third taller. `size="md"` occurs in exactly one file in the
whole frontend — `ContractDetailPage.tsx:132` (the face chip) and `:139`
(the upgradeability badge). Every other chip — asset header, account header,
all tables, transactions, ledgers — is `sm`. Every `<Chip>` in the app passes
`size` explicitly, so nothing is silently inheriting the `lg` default.

Decide one way, then apply it everywhere: either the contract page comes down
to `sm`, or page headers get a documented larger chip and the asset and
account headers go up with it. What must not survive is one page differing by
accident.

## 2. Page titles and section titles render identically

`h1` and `h2` both resolve to `heading5SemiBold` — 24px/600 — so a page title
carries no more weight than the section heading below it. Verified on two
production pages (`Ledger 63,992,485` vs `Summary`, `Contract` vs `Summary`):
identical computed size and weight. The hierarchy exists in the markup and
nowhere else. `SectionCard` is shared, so this is one change, not per page.

## 3. The footer calls `/assets` "Tokens"

`AppShell.tsx:32`. The top nav, the breadcrumb, the list page title and the
skeleton all say "Assets" (`routes.ts:62` and three page files). One label
out of five.

## 4. The NFT page uses a different empty marker and a different section name

- `NftMetadata.tsx:29` and `:105` render `N/A`; the em dash `—` is used in 43
  places across the rest of the app.
- `NftSummary.tsx:70` titles its first section `Details`; account, asset,
  contract, pool and ledger all title theirs `Summary`.

## 5. One ledger field, two names

`Closed at` as the column header on the ledgers list (`LedgersTable.tsx:56`),
`Timestamp` as the row label on ledger detail (`LedgerSummary.tsx:115`).

## 6. Duplication with no visual symptom

Cheap to fold in while the files are open; skip any that turn out to be
load-bearing.

- Two components render the same section header: `SectionCard` (account,
  asset, contract, pool) vs `TableSectionHeader` (ledger, NFT). Overlaps
  task 0366, which is migrating detail tables onto the shared card — check
  0366 first and leave this to it if it already lands there.
- `BREADCRUMB_TRUNCATION` is declared in two files with a value identical to
  the shared `DEFAULT_TRUNCATION`.
- `ExecutionTrace.tsx:267` and `:289` hand-roll the 4…4 middle truncation the
  shared helper already does.
- `humanizeOp.ts` calls raw `toLocaleString('en-US')` three times, though
  `formatInteger` documents itself as the canonical replacement for exactly
  that.

## Not in scope

- **What the SAC chip says** (`Stellar Asset Contract` on contract detail vs
  `SAC` on five other surfaces). That belongs to 0483, which is already
  rewriting what the label says — deciding the wording twice, in two tasks,
  is how the register drifts in the first place.
- Anything requiring an API change. Every item here is frontend-only.

## Acceptance criteria

- [ ] One chip-size rule, applied to every surface, with the decision written
      down where the next person will find it
- [ ] Page title is visually distinct from section titles
- [ ] `/assets` has one name across nav, footer, breadcrumb and page title
- [ ] One empty-value marker app-wide; NFT section title matches its siblings
- [ ] The ledger close timestamp has one label on both surfaces
- [ ] vitest cases only where a rule is worth pinning — not one test per label
- [ ] **Docs updated** — `docs/architecture/frontend/frontend-overview.md` if
      the chip-size rule becomes a documented convention; `N/A` otherwise
- [ ] **API types regenerated** — N/A, frontend-only

## Notes

The audit also produced one retracted finding worth recording so it is not
re-reported: the contract detail loading skeleton appeared to promise a KPI
strip the page never renders. It does render one (`ContractSummary`); only
the labels differ (`Total invocations` in the skeleton, `Invocations (last 7
days)` on the page). Worth aligning while in the file, not worth its own line
item above.

The same audit found a real data defect, filed separately as 0487 — it is a
wrong number, not a style drift, and does not belong in this sweep.
