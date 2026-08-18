---
name: release
description: Cut a production release — collect what actually ships since the last tag, draft the develop→master PR, hand over the exact tag command, then verify on production and chain into /issues. Use when shipping to production, cutting a release, or asking "what is in this release?".
---

# /release — collect what ships, then cut the tag

A release is **a `production-YYYY.MM.DD-N` tag on `master`**. This skill
assembles what that tag will ship, drafts the PR that gets it onto `master`,
hands the operator the tag command, and picks the verification back up once the
run is green.

`/release` — full pass
`/release --since <ref>` — override the range start (first release, or a
missed tag)

## Ground rules

1. **Never push the tag and never merge the PR.** Both are the human's act —
   the tag _is_ the release decision (`docs/deployment.md`), so an agent
   pushing it removes the only approval gate the pipeline has.
2. **Never write `Closes/Fixes/Resolves #N`.** Always `Refs #N`; issues close
   at deploy, and those keywords fire on merge to `master`.
3. **Do not claim an issue is shipped because its `Refs` appears in the
   range.** See _Step 3_ — this is the step most likely to produce a wrong
   close.
4. **Do not claim a crate is deployed because it merged.** See _Step 2_.

## Step 1 — the range

```bash
git fetch origin --quiet
git tag --list 'production-*' --sort=-creatordate | head -3
```

Range is **last `production-*` tag → `origin/develop`** before the release PR
exists, and → `origin/master` once it is merged. Say which one you used.

No previous tag (first release) → stop and ask for `--since`. Do **not** fall
back to all history; a release note listing 400 commits is worse than none.

```bash
git log --oneline <last-tag>..origin/develop --no-merges
git diff --shortstat <last-tag>...origin/develop
```

## Step 2 — separate what deploys from what merely merged

**This is the step that stops a false "it's live".** The tag's suffix picks the
scope, so the honest answer depends on which tag you are about to hand over —
decide the scope here, in Step 2, not at Step 5:

| Tag                                 | CDK                           | SPA |
| ----------------------------------- | ----------------------------- | --- |
| `production-<date>-<N>`             | `Explorer-production-Compute` | yes |
| `production-<date>-<N>-all`         | every stack that differs      | yes |
| `production-<date>-<N>-<StackName>` | that stack, `--exclusively`   | no  |
| `production-<date>-<N>-web`         | nothing                       | yes |

The mapping is `infra/scripts/deploy-scope.sh` — run it rather than reasoning
about the workflow's `if:` conditions:

```bash
./infra/scripts/deploy-scope.sh production-2026.08.18-1-CloudWatch
```

**Default to the plain tag.** `-all` is the only path that can ship drift
nobody reviewed — a delta parked by another task goes out as a stowaway — so
reach for it only after reading the diff, and prefer naming the one stack you
mean. The single-stack form is also the answer when infra changed and code did
not: two tags, each with its own diff, beats one `-all` nobody can audit.

Compute is exactly three Lambdas — check, do not recall:

```bash
grep -n "new RustFunction" infra/src/lib/stacks/compute-stack.ts
```

Today: the API, the Ledger Processor (indexer) and the Type-1 enrichment
worker. Anything else in `crates/**` **is not deployed by the tag**.
`backfill-runner` in particular is a binary built by hand on the box, so its
fixes ride the release into the repo and reach production only at the next
manual build there. Say so explicitly in the release note — a green tag has
been read as "the incident fix is live" when it was not.

The same question applies to `infra/**`. Diff it against the scope you chose,
and list every stack the tag leaves behind:

```bash
git diff --stat <last-tag>...origin/develop -- infra/src
```

One trap worth stating in the note: **the SPA step is not a Delivery deploy.**
It reads that stack's `SpaBucketName` / `DistributionId` outputs and syncs S3,
nothing more. So a change to `delivery-stack.ts` — cache policy, the CloudFront
function, the cert — does _not_ ship on a plain tag no matter how green the
frontend smoke is. It needs `-Delivery` or `-all`.

Also flag the inverse: a crate whose _behaviour_ does not change even though
its binary does. Adding a `pub fn` to a shared crate relinks the indexer
without changing what it does. Worth one line, so the diff's size does not
imply risk it does not carry.

```bash
git diff --stat <last-tag>...origin/develop -- ':!lore'
```

Schema changes deserve their own line. `grep` the range for `CREATE TABLE` /
`ALTER TABLE` and state plainly whether anything must exist on production CH
_before_ the code lands — that ordering has bitten before
(`docs/backfills.md`).

## Step 3 — collect issues, then check each one

Per merged PR in the range: number, title, `lore-NNNN` scopes, `Refs #NNN`.

```bash
git log <last-tag>..origin/develop --format='%s%n%b' | grep -oE 'Refs #[0-9]+' | sort -u
```

**Then open each referenced issue and check the range actually advances it.**
A `Refs` trailer is a claim, not evidence, and both failure modes are real:

- a **docs-only** commit referencing an issue no code touched (a task file
  linking `#405` does not index Soroban AMMs);
- a **wrong trailer** — a commit about disk headroom on the box carrying
  `Refs #371`, an issue about a transactions view.

Bucket the referenced issues into **shippable** (code in this range plausibly
resolves the ask), **partial** (bundled issue, some tasks shipped) and
**noise** (referenced but not advanced). Only the first two reach Step 6, and
partials never close.

## Step 4 — draft the release PR

Base `master`, head `develop`. Title:
`release: <the one or two things a reader would recognise>`.

Body sections, in this order:

- **What ships** — one block per lore task that carries code, with the issue
  it advances. Bookkeeping and lore-only changes get one collapsed line at the
  end, not a block each.
- **What does not deploy from this tag** — from Step 2. Omit the section only
  if there is genuinely nothing.
- **Deploy order / prerequisites** — table pre-creates, backfills,
  API-before-SPA. "None" is a valid and useful answer.
- **Issues** — shippable, partial, and the noise called out as noise.

Then create it, and print the URL:

```bash
gh pr create --base master --head develop --title "<title>" --body-file <file>
```

This is deliberately **not** `/pr`. That skill derives one PR from the active
lore task; a release PR spans many tasks and has no active task of its own.

## Step 5 — the tag

After the human merges. `-N` is the release counter **for that date**:

```bash
git fetch origin --quiet && git tag --list "production-$(date -u +%Y.%m.%d)-*"
```

Hand over the command — do not run it. Append the Step 2 selector, or nothing
for the standard Compute + SPA release:

```bash
git fetch origin && git tag production-<YYYY.MM.DD>-<N> origin/master && git push origin production-<YYYY.MM.DD>-<N>
```

An infra-only release names its stack, and shipping both halves is two tags,
not one — separate diffs, separate blast radii, and `-N` just increments:

```bash
git fetch origin && git tag production-<YYYY.MM.DD>-<N>-CloudWatch origin/master && git push origin production-<YYYY.MM.DD>-<N>-CloudWatch
```

Four details that matter:

- **`origin/master`, not `master`.** A worktree's local `master` is routinely
  stale, and the environment's tag policy will happily deploy an old commit.
- **Push the tag by name, not `--tags`**, which also pushes every stale local
  tag.
- **The workflow runs at the tagged commit.** Fixing the workflow means a new
  tag, never a re-run. This includes the selector grammar itself: a tag cut
  before the selector shipped deploys the old way, whatever its name says.
- **The selector is case-sensitive** — it is pasted onto `Explorer-production-`
  verbatim. `-cloudwatch` fails in `cdk`; `-CloudWatch` is the stack. A typo
  fails before anything deploys, which is the intended failure mode, but it
  costs a whole run.

## Step 6 — watch, then verify

```bash
gh run watch --exit-status $(gh run list --workflow deploy-production.yml --limit 1 --json databaseId --jq '.[0].databaseId')
```

**Budget ~15 minutes; expect less, promise nothing.** `cdk diff` builds all
three Lambdas during synth. Since 2026-08-18 the tag run restores the Rust
cache CI writes on every `master` push (same arm runner, same `ci-rust-lambda`
key), so a warm run compiles only the workspace crates — but the cache can be
evicted, and a cold run is the old ~11-minute diff. A `-web` tag skips the
build entirely. Full reasoning in `docs/deployment.md` § Releases.

**Read the `cdk diff` step even on a green run.** It covers _every_ stack, not
just the deployed ones (`-web` runs are the exception — they skip the build and
print no diff), which makes it the one place a delta parked in an undeployed
stack becomes visible. Anything it lists outside your tag's scope is
still-not-live: either cut a second tag for it or say so in the note. Silence
here is the honest signal that nothing is left behind.

**Green is not verified.** Two traps, both hit in practice:

- The SPA smoke asserts HTTP 200, and CloudFront invalidation is fired without
  waiting — so the check can pass against the previous bundle.
- **Verify from the surface that changed.** If the fix changed what the API
  answers, prove it from the API or the page, not from a SQL count in
  ClickHouse — the store was never wrong, so querying it proves nothing. Open
  the reporter's own URL where there is one, and cross-check a concrete number
  against Horizon (`/transactions/<hash>/effects`) when amounts are involved.

## Step 7 — hand off

Run `/issues`. Give it the Step 3 buckets so its Step 4 reads this release
instead of asking what was deployed. It drafts the comments; a human posts
them.

Finally, close out the lore tasks whose acceptance criteria were
deploy-gated — `/lore-framework-tasks`, and tick them against the verification
from Step 6, not against the deploy having happened.
