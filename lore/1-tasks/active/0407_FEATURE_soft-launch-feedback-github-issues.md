---
id: '0407'
title: 'FEATURE: soft-launch feedback — header "Report a bug" link → GitHub issue-form template'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags: [priority-medium, effort-small, layer-frontend, phase-launch]
milestone: 3
links:
  - .github/ISSUE_TEMPLATE/bug_report.yml
  - .github/ISSUE_TEMPLATE/config.yml
  - libs/ui/src/layout/SecondaryNav.tsx
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
  - date: 2026-07-22
    status: active
    who: karolkow
    note: >
      Placement reversed footer -> header after marketing (Aga) asked for
      one canonical destination to point soft-launch users at, citing
      Voyager's separated top-right "Report a bug". Footer entry removed —
      one affordance, not two. Shipped as 09acbbd2 + e16fb0a1 (drawer
      sizing). The issue form + user-facing guide are still open.
---

# FEATURE: soft-launch feedback — header "Report a bug" → GitHub Issues

## Summary

Give the first soft-launch users a one-click path to report bugs, routed to
the project's public GitHub Issues. Add a structured issue-form template so
reports arrive with the context we need (page URL, what happened, expected),
and a header link that opens it.

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
- **Placement: header** — originally the footer (user's explicit choice), then
  reversed on 2026-07-22 at marketing's request; see Emerged decision 4. The
  footer entry is gone, so there is exactly one affordance.

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
3. **`libs/ui/src/layout/SecondaryNav.tsx`** — `Report a bug ↗` in the
   top-right cluster, divided from the nav tabs, next to the theme toggle;
   below `md` it moves into the hamburger drawer. Href
   `https://github.com/rumblefishdev/soroban-block-explorer/issues/new?template=bug_report.yml`
   once the form exists (today: bare `issues/new`).
   **`libs/ui/src/layout/Footer.tsx`** — the `Report an issue` entry is
   removed; the `GitHub` (source) link stays.

Prefill safe params only: `template`, and optionally `url` (text field).
`dropdown` prefill is not supported by GitHub — leave type unset.

## Acceptance Criteria

- [x] Header shows a `Report a bug` link, separated from the nav tabs
      (`09acbbd2`); drawer variant matches its neighbours' sizing (`e16fb0a1`)
- [ ] `bug_report.yml` renders as a form; auto-applies `soft-launch` label
- [ ] `config.yml` disables blank issues; general questions routed elsewhere
      — **not Discussions**: they are disabled on the repo (verified
      `gh repo view`), so pick another contact link
- [ ] Header href carries `?template=bug_report.yml` once the form is on the
      default branch (GitHub reads templates from `master` only)
- [ ] User-facing guide on how to file an issue (asked for on Slack)
- [x] FE tests / typecheck / lint green (pre-commit hooks on both commits)

## Design Decisions

### From Plan

1. **Issues over Discussions on intake**: user chose Issues — reports land
   directly in the tracker (accepting some low-value noise) rather than a
   triage buffer.
2. **Footer over header**: user's explicit choice — **superseded**, see
   Emerged decision 4.
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

- Per-page prefill: `&url=<current href>` on the header link so the reporter
  does not paste the address by hand (needs router context inside
  `SecondaryNav`, which today takes none).
