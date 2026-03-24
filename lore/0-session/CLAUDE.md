# Session Directory

Local session state. Files are gitignored except `team.yaml`.

| File | Purpose |
|------|---------|
| `current-user.md` | Who is working |
| `current-task.md` | Symlink to active task |
| `current-task.json` | Task metadata (id + path) |
| `team.yaml` | Team data (source of truth) |
| `next-tasks.md` | Available tasks by priority |

## Automatic Setup

Configure `LORE_SESSION_CURRENT_USER` in `.claude/settings.local.json`:

```json
{
  "env": {
    "LORE_SESSION_CURRENT_USER": "your-id"
  }
}
```

The session start hook auto-generates `current-user.md` on startup.

## Manual Setup

Use MCP tools:
- `lore_set-user` — Set current user
- `lore_set-task` — Set current task by ID
- `lore_list-users` — List available users
- `lore_show-session` — Show current session state
