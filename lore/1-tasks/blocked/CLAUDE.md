# Blocked Tasks

Tasks waiting on dependencies.

## Rules

- Must specify what's blocking in history entry
- Use `by: ["NNNN"]` for task dependencies
- Use `note:` for external blockers (not task-related)

## Frontmatter Example

```yaml
history:
  - date: 2025-01-20
    status: blocked
    who: claude
    by: ["0007", "0008"]
    note: "Waiting on domain models and schema"
```

## Unblocking

When blocker resolves, move back to active:
```bash
git mv blocked/NNNN_*.md active/
```
Update frontmatter: `status: active`, add history entry.
