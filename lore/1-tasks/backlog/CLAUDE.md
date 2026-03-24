# Backlog

Future work, not yet started.

## Format

Same as tasks: `NNNN_TYPE_slug.md`

Use `status: backlog` in frontmatter.

## Tags

| Category | Values |
|----------|--------|
| Priority | `priority-high`, `priority-medium`, `priority-low` |
| Effort | `effort-small`, `effort-medium`, `effort-large` |

Add project-specific tags as needed.

## Promotion

When ready to start:
```bash
git mv backlog/NNNN_*.md active/
```
Update frontmatter: `status: active`, add history entry.
