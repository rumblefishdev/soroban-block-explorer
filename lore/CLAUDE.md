# Lore Directory

Context persistence for stateless Claude sessions.

## IMPORTANT: Required Skills

**You MUST use the following skills when working in the `lore/` directory. This is not optional.**

| Skill | When to use | Invoke with |
|-------|-------------|-------------|
| `lore-framework` | General lore work, notes, workflows, session management | `/lore-framework` |
| `lore-framework-git` | ALL git commits (Conventional Commits + task references) | `/lore-framework-git` |
| `lore-framework-tasks` | ALL task updates: status changes, completion, spawning follow-ups | `/lore-framework-tasks` |

**Never update task status, complete tasks, or create follow-up tasks without invoking `/lore-framework-tasks` first.**
**Never make git commits without invoking `/lore-framework-git` first.**

## Session Files

| File | Purpose | Committed |
|------|---------|-----------|
| `0-session/current-user.md` | Who is working (generated) | No |
| `0-session/current-task.md` | Symlink to active task | No |
| `0-session/current-task.json` | Task metadata for agents (id + path) | No |
| `0-session/team.yaml` | Team data (source of truth) | Yes |
| `0-session/next-tasks.md` | Available tasks (auto-generated) | No |
| `README.md` | Full index with Mermaid (heavy) | Yes |

**Before coding:** Ensure `0-session/current-user.md` and `0-session/current-task.md` exist.

**Setup:** Use MCP tools `lore_set-user` and `lore_set-task`

## Structure

Each subdirectory has `CLAUDE.md` with local context.

```
lore/
├── 0-session/CLAUDE.md    # Session state
├── 1-tasks/CLAUDE.md      # Task system
│   ├── backlog/CLAUDE.md
│   ├── active/CLAUDE.md
│   ├── blocked/CLAUDE.md
│   └── archive/CLAUDE.md
├── 2-adrs/CLAUDE.md       # ADR frontmatter
└── 3-wiki/CLAUDE.md       # Project docs
```

## Quick Reference

**Format:** `NNNN_TYPE_slug.md` (shared ID sequence for tasks + backlog)

**Task lifecycle:** `backlog/` → `active/` → `blocked/` ↔ `active/` → `archive/`

**Note prefixes:** Q- (Question), I- (Idea), R- (Research), S- (Synthesis), G- (Generation)

**Templates:** Use `_template.md` in each directory.

## Full Documentation

All system docs are in the `/lore-framework` skill.
