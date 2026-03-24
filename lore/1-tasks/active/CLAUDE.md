# Active Tasks

Tasks currently being worked on.

## Rules

- One task active per person at a time (focus)
- Update status in frontmatter when moving
- Add history entry on status change

## Moving Tasks

**To blocked:**
```bash
git mv active/NNNN_*.md blocked/
```
Update frontmatter: `status: blocked`, add `by: ["blocking-task-id"]`

**To archive:**
```bash
git mv active/NNNN_*.md archive/
```
Update frontmatter: `status: completed`
