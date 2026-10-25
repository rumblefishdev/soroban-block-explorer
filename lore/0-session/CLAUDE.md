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

Use the `lore-framework` plugin's MCP tools. Each is listed to an agent under
its full name, `mcp__plugin_lore-framework_lore-framework__lore-framework_<verb>`
— the short names below are this repo's shorthand, not callable as written:
- `set-user` — Set current user
- `set-task` — Set current task by ID
- `list-users` — List available users
- `show-session` — Show current session state
