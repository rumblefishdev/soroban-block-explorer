---
id: '0274'
title: 'Figma: mockups for pages shipped without a Figma design (Accounts list)'
type: DOCS
status: backlog
related_adr: []
related_tasks: ['0226', '0273']
tags: [priority-low, effort-small, layer-design, phase-pre-launch, milestone-2]
milestone: 2
links:
  - web/src/pages/AccountsListPage.tsx
  - web/src/pages/accounts/AccountsTable.tsx
  - web/src/pages/accounts/AccountsFilters.tsx
history:
  - date: '2026-05-29'
    status: backlog
    who: FilipDz
    note: >
      The Accounts list page was built from a colleague's text spec
      (page header / card controls / 7-column table) without a Figma
      source. Every other explorer page has a Figma node id baked
      into its `// Figma:` comments — this one is the odd one out.
      Need a Figma frame so design parity + future iteration have an
      anchor.
---

# Figma: mockups for pages shipped without a Figma design

## Summary

Create a Figma frame for the Accounts list page so it has the same
design anchor every other explorer page has. The current
implementation was built from a colleague's text spec only; without
a Figma reference there's nothing to design-parity-audit against,
and future tweaks have no canonical source.

## Context

During the design-parity pass we walked every page against its
Figma node and corrected typography / colours / spacing
mismatches. The Accounts list was the exception — it was built
fresh from a Polish text spec ("page header / card controls /
table columns: # | Account | XLM Balance | % Supply | Last Seen |
First Seen | Domain / pagination") with no Figma frame.

The page works and matches the colleague's intent, but:

- No Figma node id for the `// Figma:` typography comments.
- No reference for follow-up visual tweaks.
- Risk of drift from the design-system patterns established on
  sibling pages.

## Implementation

- Build a Figma frame for `/accounts` in the Designs file
  (`n1p6WCMVd4iinbuvOA2WjP`, "Accounts" page or equivalent).
  Reuse existing DS components — `PageHeader`, `DataListCard`,
  `ExplorerTable` rows, `Chip` (sort + With-domain toggle),
  `IdentifierWithCopy`, etc.
- Mirror the live implementation
  ([`AccountsListPage.tsx`](../../../web/src/pages/AccountsListPage.tsx),
  [`AccountsTable.tsx`](../../../web/src/pages/accounts/AccountsTable.tsx),
  [`AccountsFilters.tsx`](../../../web/src/pages/accounts/AccountsFilters.tsx))
  with: page header (title + subtitle), filter row (search + sort
  Select + With-domain Chip), 7-column table (#, Account, XLM
  Balance, % Supply, Last Seen, First Seen, Domain), pagination
  footer with the per-sort caption.
- After the frame exists, backfill the `// Figma:` comments in the
  three FE files with the node ids.
- (Optional, follow-up) Spec mobile breakpoints — the current
  responsive behaviour was applied per the same heuristic used on
  the other list pages; a Figma reference would let design
  validate or override.

## Acceptance Criteria

- [ ] Figma frame for the Accounts list page exists in the Designs
      file, using DS components consistently with the other list
      pages.
- [ ] FE `// Figma:` comments reference the new node id(s).
- [ ] (Optional) Figma frame for the mobile breakpoint exists.

## Notes

- This task is doc-shaped (design artefact), not implementation.
  No FE code changes required beyond the comment backfill.
- If other pages turn out to also lack a Figma source, expand the
  scope rather than spawning sibling tasks.
