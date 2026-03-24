# Tasks

All tasks by lifecycle status. Shared ID sequence (NNNN).

## IMPORTANT: Always use `/lore-framework-tasks` skill

**When updating tasks (status changes, completing, archiving, spawning follow-ups), you MUST invoke the `/lore-framework-tasks` skill FIRST.** This is critical for maintaining consistent task lifecycle and documentation. Never update task status manually without the skill.

## Directories

- [backlog/](backlog/CLAUDE.md) — future work, not yet started
- [active/](active/CLAUDE.md) — currently in progress
- [blocked/](blocked/CLAUDE.md) — waiting on dependencies
- [archive/](archive/CLAUDE.md) — completed

## Format

`NNNN_TYPE_slug.md` or `NNNN_TYPE_slug/` (directory for complex tasks)

## Task Size

**Keep README.md short** (~50-100 lines): summary, status, context.

Heavy content goes into `notes/` subdirectory. **Convert to directory when task grows beyond ~150 lines.**

## Note Prefixes

| Prefix | Type | Use for |
|--------|------|---------|
| `Q-` | Question | What we're trying to answer |
| `I-` | Idea | Original thoughts, hypotheses |
| `R-` | Research | External knowledge (papers, docs, analysis) |
| `S-` | Synthesis | Conclusions, decisions ("so what?") |
| `G-` | Generation | Artifacts we produce (specs, schemas, designs) |

Lineage via `spawned_from`/`spawns`. Status: `seed → developing → mature → superseded`.

Create: `_note_template.md` | Full docs: `/lore-framework` skill

## Lifecycle

```
backlog/ → active/ → blocked/ ↔ active/ → archive/
```

Promotion: `git mv` between directories, update `status` in frontmatter.

## Templates

- `_template.md` — new tasks
- `_note_template.md` — notes in task `notes/` directories
