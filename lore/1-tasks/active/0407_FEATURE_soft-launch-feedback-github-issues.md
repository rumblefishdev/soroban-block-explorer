---
id: '0407'
title: 'FEATURE: soft-launch feedback — footer "Report an issue" link → GitHub issue-form template'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags: [priority-medium, effort-small, layer-frontend, phase-launch]
milestone: 3
links:
  - .github/ISSUE_TEMPLATE/bug_report.yml
  - .github/ISSUE_TEMPLATE/config.yml
  - libs/ui/src/layout/Footer.tsx
history:
  - date: 2026-07-17
    status: active
    who: karolkow
    note: >
      Created for soft launch. First external users need a way to report
      bugs. Decision: route to public GitHub Issues (repo is public,
      Issues enabled), affordance in the footer (user's explicit choice
      over a header element). Research (2 agents, live DOM + repo reads):
      blockchain explorers barely solve this (etherscan/solscan = own form,
      stellar.expert's "Report a bug" links to a repo with Issues DISABLED,
      stellarchain has nothing); dev-tools converge on
      issues/new?template=<form>.yml with a structured .yml issue form and
      blank_issues_enabled:false (Zed, n8n, Vercel, 11/12 surveyed).
---

# FEATURE: soft-launch feedback — footer "Report an issue" → GitHub Issues

## Summary

Give the first soft-launch users a one-click path to report bugs, routed to
the project's public GitHub Issues. Add a structured issue-form template so
reports arrive with the context we need (page URL, what happened, expected),
and a footer link that opens it prefilled.

## Context

Soft launch imminent. Repo `rumblefishdev/soroban-block-explorer` is **public
with Issues enabled** (verified `gh repo view`). Today the only affordance is
a footer link labeled "GitHub" ([Footer.tsx](libs/ui/src/layout/Footer.tsx)
`RESOURCES[0]`) that points at the repo root — not an invitation to report,
and it lands on a blank free-text issue with no context.

**Decisions made with the user:**

- **Destination: GitHub Issues** (not Discussions, not own backend/widget).
  Audience is Stellar/Soroban devs who already have GitHub; a public tracker
  also signals building-in-the-open. Zero backend, zero new dependency.
- **Placement: footer** (user's explicit choice over a header element). Matches
  every explorer convention; lower discoverability than a header chip is the
  accepted trade-off.

## Implementation

1. **`.github/ISSUE_TEMPLATE/bug_report.yml`** — GitHub issue form. Fields:
   - `dropdown` type: Bug / Wrong data / Suggestion
   - `input` id `url` — the page it happened on (prefillable via query param)
   - `textarea` what happened (required)
   - `textarea` what you expected
   - `labels: [soft-launch, needs-triage]` set **in the yml**, NOT via URL —
     the `?labels=` param 404s for users without repo write access
     (GitHub docs: permissions required per query param).
2. **`.github/ISSUE_TEMPLATE/config.yml`** — `blank_issues_enabled: false`;
   `contact_links` → GitHub Discussions for general questions/ideas.
3. **`libs/ui/src/layout/Footer.tsx`** — add a `Report an issue` entry to
   `RESOURCES`, href
   `https://github.com/rumblefishdev/soroban-block-explorer/issues/new?template=bug_report.yml`.
   Keep the existing `GitHub` (source) link.

Prefill safe params only: `template`, and optionally `url` (text field).
`dropdown` prefill is not supported by GitHub — leave type unset.

## Acceptance Criteria

- [ ] Footer shows a `Report an issue` link opening the bug-report form
- [ ] `bug_report.yml` renders as a form; auto-applies `soft-launch` label
- [ ] `config.yml` disables blank issues; general questions routed to Discussions
- [ ] Existing `GitHub` (source) footer link preserved
- [ ] FE tests / typecheck green

## Design Decisions

### From Plan

1. **Issues over Discussions on intake**: user chose Issues — reports land
   directly in the tracker (accepting some low-value noise) rather than a
   triage buffer.
2. **Footer over header**: user's explicit choice.
3. **Labels in yml, not URL query**: `?labels=` 404s for non-collaborators;
   the form template applies labels server-side regardless of who submits.

### Emerged

4. **Header, not footer — decision reversed 2026-07-22.** Marketing (Aga)
   needs one canonical place to point soft-launch users at; footer intake is
   too easy to miss. New placement copies Voyager: `Report a bug ↗` in the
   top-right cluster of `SecondaryNav`, separated from the nav tabs by a
   divider, next to the theme toggle. Footer entry **removed** — one
   affordance, not two.
5. **Label `Report a bug`** (Voyager wording), even though the planned issue
   form also covers wrong-data / suggestions.
6. **Link is hardcoded in `SecondaryNav`, not a prop** — same convention as
   `RESOURCES` in `Footer.tsx`; no new prop threading through the app shell.
7. **Mobile**: the link does not stay inline below `md` (would squeeze the
   nav row); it renders at the bottom of the hamburger drawer, under a
   separator.

## Future Work

- Per-page "Report an issue" with `&url=<current href>` prefill if footer
  intake proves too low-signal (would need router context; header/per-page,
  out of this footer-only scope).
