---
id: '0479'
title: 'Board parser drops multi-line tags and skips unparseable tasks in silence'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0290', '0310']
tags: ['phase-future', 'effort-small', 'priority-medium', 'layer-tooling']
links: []
history:
  - date: 2026-08-13
    status: backlog
    who: karolkow
    note: >
      Spawned while repairing the frontmatter of 0290 and 0310, which had
      both lost the closing `---` and so were absent from the published
      board. Repairing the two files restored them, but exposed two
      independent weaknesses in `generate-lore-board.mjs` that the repair
      does not address. Both fixes were written and measured during that
      session, then deliberately left out of the commit to keep it to the
      markdown repair.
---

# Board parser drops multi-line tags and skips unparseable tasks in silence

## Summary

`tools/scripts/generate-lore-board.mjs` parses task frontmatter with a
hand-rolled line-by-line reader. It only understands a flow sequence written on
one line, so every `tags:` array that the formatter has wrapped is read as
empty — and the board derives **layer** and **priority** from those tags.
Separately, a file whose frontmatter does not parse at all is skipped without
a word, so a broken task disappears from the board while the build stays green.

## Context

The board is generated from `develop` on every push
(`.github/workflows/deploy-board.yml`) and published to GitHub Pages. Its UI
offers a layer filter, a priority filter, sortable `Layer` / `Priority` columns
and per-layer card colours — all fed by the tags.

Prettier wraps any line over 80 characters, which splits a longer
`tags: [...]` across several lines. `parseFrontmatter` matches a key with an
inline array, or an indented `- item` list; the wrapped `[` line matches
neither, so the value stays empty. `getLayer` then falls back to `layer-other`
and `getPriority` to `medium`.

Measured on the tree at the time of writing (462 tasks):

|                                       | before | after a prototype fix |
| ------------------------------------- | ------ | --------------------- |
| tasks carrying tags                   | 281    | 462                   |
| tasks without a layer (`layer-other`) | 264    | 170                   |
| priority `high`                       | 104    | 172                   |
| priority `low`                        | 51     | 68                    |

So 68 tasks marked `high` in their own file render as `medium`, 17 marked `low`
do the same, and 94 lose their layer. The filters silently return an incomplete
set rather than failing — the board looks right and is not.

The second weakness is what made this hard to find. Tasks 0290 and 0310 had
lost the closing `---` of their frontmatter, so the regex in `parseFrontmatter`
did not match, `loadTasks` hit `if (!meta || !meta.id) continue`, and both
vanished from the board with no warning and a successful workflow run. What
removed those delimiters is still unknown — Prettier was ruled out by test,
with neither the bare binary nor the repo config touching them — so the same
corruption can recur.

## Implementation

1. **Fold wrapped flow sequences back onto the key line** before the parse loop
   in `parseFrontmatter`, e.g. a `replace` over the frontmatter block matching
   `^key:\n<indent>[ ... ]` and re-emitting `key: [ ... ]`. The rest of the
   parser already handles the inline form. Prototype measured at 181 tasks
   regaining tags with zero regressions.
2. **Fail on unparseable frontmatter** in `loadTasks`: collect the offending
   paths and exit non-zero with them listed, instead of `continue`.
   - Guard against a false positive: files with no frontmatter _at all_ are
     companion documents, not tasks — `lore/1-tasks/archive/0339_phase2-migration-runbook.md`
     is one. Only flag a file that opens with `---` and still fails to parse.
   - Accept the trade-off, or argue it down to a warning: with a hard failure
     one bad file blocks the board deploy entirely.

Both changes sit in `tools/scripts/generate-lore-board.mjs`; nothing else is
touched.

## Acceptance Criteria

- [ ] A task whose `tags:` array is wrapped across lines reports its real layer
      and priority on the board
- [ ] Board task count and per-task tags are unchanged for tasks that already
      parsed (no regressions)
- [ ] A task file that opens with `---` and fails to parse makes `npm run board`
      exit non-zero and name the path
- [ ] A file in `lore/1-tasks/**` with no frontmatter is still skipped quietly
- [ ] **Docs updated** — `N/A` — build tooling only, does not change the shape
      of the system described in `docs/architecture/**`
- [ ] **API types regenerated** — `N/A` — touches no Rust crate, no
      `Cargo.{toml,lock}`, no `libs/api-types/**`

## Notes

Two data smells found while measuring, both out of scope here and neither
blocking anything:

- `lore/1-tasks/archive/0339_phase2-migration-runbook.md` carries no
  frontmatter and no type segment in its filename, though it sits in the task
  directory. It reads as a companion runbook that belongs under the `notes/`
  of its task.
- `0320_BUG_wasm-upgrade-not-reclassified` tags itself `priority-normal`, a
  value outside `high` / `medium` / `low` / `critical`. The board will render
  `normal` verbatim as a priority.
