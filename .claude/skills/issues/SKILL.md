---
name: issues
description: Reconcile GitHub issues against lore tasks and what is actually deployed — triage new reports, spot duplicates, draft reply/merge/close comments for the user to post. Use when new issues arrive, after merging a PR that references one, and after every production deploy.
---

# /issues — reconcile GitHub issues with lore tasks and production

Reads the current state and reports what needs a human action. Takes no mode:
run it whenever, it works out which bucket each issue is in.

`/issues` — full pass
`/issues 366` — one issue
`/issues web` — narrow the deployed-check to one component

## Ground rules — do not break these

1. **Never post, edit, label or close anything on GitHub.** Every comment is a
   draft handed to the user, who posts it. `gh` is read-only here: `list`,
   `view`, `search`. No `gh issue comment/close/edit`, no `gh pr comment`.
2. **Issues close at deploy, never at merge.** Merged code is not shipped code.
3. **Never write `Closes/Fixes/Resolves #N`** in a PR body, commit or comment —
   GitHub auto-closes on merge to `master`, which is exactly the wrong moment.
   Always `Refs #N`.
4. **Link is one-way.** The lore task carries the issue URL in `links:`. Issue
   comments do not mention lore paths or task IDs.
5. **Do not claim a fix works because it merged.** See _Verify_ below.

## Step 1 — read the state

```bash
gh issue list --state open --limit 50 \
  --json number,title,labels,createdAt,author,body
```

For each open issue find its lore task. **Resolve against `develop`, never
against whatever happens to be checked out** — you are likely sitting in a
worktree on a feature branch that does not have the recent tasks at all, and a
plain `grep` there returns a screen of confident false negatives:

```bash
git fetch origin develop --quiet
git grep -l "issues/<N>" FETCH_HEAD -- lore/1-tasks/
```

An issue with no task is untriaged. A task with no `links:` entry that clearly
came from a report is a broken link — flag it. Before concluding that nothing
is linked, confirm the tree you searched actually contains the tasks you expect.

Directory-form tasks match as `NNNN_TYPE_slug/README.md`, so take the ID from
the parent directory, not the filename — otherwise the report says `README.md`
where it means `0199`.

Then bucket every issue and report the buckets before doing anything else:

| Bucket          | Meaning                              | Action                 |
| --------------- | ------------------------------------ | ---------------------- |
| **Untriaged**   | no linked lore task                  | Step 2                 |
| **Waiting**     | task exists, not merged              | nothing — just list it |
| **Merged**      | linked PR merged, not deployed       | Step 3                 |
| **Deployed**    | fix is on production                 | Step 4                 |
| **Backfilling** | code shipped, data still catching up | Step 5                 |

## Step 2 — triage a new issue

**a. Look for duplicates first.** Cheapest thing that prevents the worst
outcome (two tasks, two answers, contradicting each other):

```bash
gh issue list --state all --search "<2-3 distinctive keywords>" \
  --json number,title,state
grep -ril "<keyword>" lore/1-tasks/
```

Three outcomes:

- **Duplicate of an open issue** → draft a comment pointing at the original;
  add this issue's URL to the same lore task's `links:` (a task may link many
  issues — that is the intended shape).
- **Already declined** → the reason is in the closed issue's own thread, and
  the technical version in the owning task's `## Rejected` section. Draft a
  reply that restates that reason; do not re-litigate it from scratch, and do
  not quietly reverse it either — if the new report brings an argument the
  original decline did not consider, say so and reopen the question.
- **New** → continue.

**b. Is the report even right?** Check the claim against the code before
accepting it. Reporters are often right about the symptom and wrong about the
cause — and sometimes the thing they want already exists somewhere they did not
look. Say so plainly in the draft reply; that is the most useful answer we can
give and it costs nothing to ship.

**c. Size it.** State which and why, with `file:line`:

| Size             | Means                                                          |
| ---------------- | -------------------------------------------------------------- |
| `one-liner`      | frontend-only or a single query change, no API contract change |
| `small`          | crosses layers, needs `nx run @rumblefish/api-types:generate`  |
| `large`          | new query shape / schema change / measurement needed first     |
| `needs-backfill` | historical data must be re-processed — see Step 5              |
| `declined`       | we are not doing it                                            |

**d. Record it.**

- Doing it → lore task (invoke `/lore-framework-tasks`), issue URL in `links:`.
  Bundled issue (several asks in one) → several tasks, all linking that issue.
- Declining it → the decline needs a written reason in two registers that
  already exist, and no third one. A partial decline inside work we _are_
  doing goes in that task's `## Rejected` section (see 0440 rejecting regex).
  A wholesale decline goes in the draft close comment, so the reason lives in
  the issue thread `--state all` search will find. Do not start a separate
  declined-requests file until wholesale declines are frequent enough that
  searching closed issues actually hurts — today there are none.

**e. Draft the reply.** Answer questions, correct wrong assumptions, and say
nothing about timing. Do not elaborate on points where the reporter was simply
right and we agree — acknowledge and move on. If the issue is a bare screenshot
with no text or no needed context, the draft is a clarifying question (which page? what did you
expect?) — there is no issue form, so this is where that cost is paid.

## Step 3 — after a PR merges

The reporter should not have to guess whether silence means ignored. Draft:

> This is merged and will go out with our next production deploy — I'll
> follow up here once it's live.

Note in the report that the issue is now waiting on a deploy. **Do not close.**

## Step 4 — after a production deploy

Deploys are manual and today per-stack (`make deploy-production-*`, with the
frontend SPA synced separately). So **ask the user what they just deployed**,
then only consider issues whose fix is in that component.

> When merge-to-master plus a tag or a button becomes the deploy trigger, and
> a deploy stops being per-stack, replace this question with reading the
> release. Nothing else in this skill changes.

**Verify before drafting anything — hybrid rule:**

- **Default: reproduce the reporter's own scenario on production** and put the
  result in the draft. If they gave a URL, open that exact URL. If they gave a
  screenshot, reproduce that view.
- **Use Playwright** where the check is worth keeping as a regression test
  anyway; then reference the test instead of a one-off observation.
- If verification fails, say so in the report and **draft nothing** — the issue
  stays open. A wrongly-closed issue costs more trust than a slow one.

Then draft:

> This is live on production now — [what was verified, concretely].
> Thanks again for the report.

**Bundled issues:** close only when every linked task has shipped. Exception:
if a remaining part is declined or has no realistic path, do not hold the issue
hostage to it — draft a close that says plainly what shipped and what we are
not planning, with the reason for the latter. That comment is the record.

## Step 5 — fixes that need a backfill

A backfill lands days after the deploy, so the code shipping is not the fix
landing. If the linked task is `needs-backfill`, at deploy time draft:

> The fix is deployed. Historical data is still being re-processed, so older
> records may look unchanged for a while — I'll confirm here when it's done.

Keep the issue open until the backfill completes, then verify per Step 4.

## Report format

End every run with the buckets, counts, and what needs the user:

```
Untriaged   2  → #370, #371 (drafts below)
Waiting     3  → #363 (0443, 0380, 0444), #366 (0440), #369 (0450)
Merged      1  → #364 (0444) — draft below
Deployed    0
Backfilling 0

Drafts to post: 3   (nothing has been posted)
```
