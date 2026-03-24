# Archive

Completed tasks. Reference for context and history.

## Status Values

| Status | Meaning |
|--------|---------|
| `completed` | Done successfully |
| `superseded` | Replaced by another task (use `by:`) |
| `canceled` | Won't do (use `reason:`) |

## Canceled Reasons

- `pivot` — direction change
- `obsolete` — no longer needed
- `duplicate` — covered elsewhere

## Frontmatter Examples

**Completed:**
```yaml
history:
  - date: 2025-01-20
    status: completed
    who: claude
    note: "All acceptance criteria met"
```

**Superseded:**
```yaml
history:
  - date: 2025-01-20
    status: superseded
    who: claude
    by: ["0015"]
    note: "Replaced by consolidated task"
```

**Canceled:**
```yaml
history:
  - date: 2025-01-20
    status: canceled
    who: claude
    reason: pivot
    note: "Direction changed after review"
```
