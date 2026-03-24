# Architecture Decision Records

Documented decisions. Written **post-factum** after implementation.

## Frontmatter

```yaml
---
id: "NNNN"
title: "Decision Title"
status: proposed  # proposed | accepted | deprecated | superseded
deciders: [team-member]
related_tasks: ["0008", "0019"]
related_adrs: ["0002"]
tags: []
links: []
history:
  - date: YYYY-MM-DD
    status: proposed
    who: team-member
    note: "ADR created"
---
```

## Status Lifecycle

`proposed` → `accepted` → `deprecated` or `superseded`

| Status | Meaning |
|--------|---------|
| `proposed` | Under discussion |
| `accepted` | Active decision |
| `deprecated` | No longer recommended, no replacement |
| `superseded` | Replaced by newer ADR (use `by` in history) |

## History Entry

| Field | Required | Description |
|-------|----------|-------------|
| `date` | Yes | ISO date |
| `status` | Yes | proposed, accepted, deprecated, superseded |
| `who` | No | Team member id |
| `by` | Conditional | **Required** if `superseded` - the replacing ADR ID |
| `note` | No | What happened |

## Template

Use `_template.md` for new ADRs.
