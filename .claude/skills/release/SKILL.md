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

**This is the step that stops a false "it's live".** A tag run deploys the
Compute stack and the SPA. Compute is exactly three Lambdas — check, do not
recall:

```bash
grep -n "new RustFunction" infra/src/lib/stacks/compute-stack.ts
```

Today: the API, the Ledger Processor (indexer) and the Type-1 enrichment
worker. Anything else in `crates/**` **is not deployed by the tag**.
`backfill-runner` in particular is a binary built by hand on the box, so its
fixes ride the release into the repo and reach production only at the next
manual build there. Say so explicitly in the release note — a green tag has
been read as "the incident fix is live" when it was not.

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

Hand over the command — do not run it:

```bash
git fetch origin && git tag production-<YYYY.MM.DD>-<N> origin/master && git push origin production-<YYYY.MM.DD>-<N>
```

Three details that matter:

- **`origin/master`, not `master`.** A worktree's local `master` is routinely
  stale, and the environment's tag policy will happily deploy an old commit.
- **Push the tag by name, not `--tags`**, which also pushes every stale local
  tag.
- **The workflow runs at the tagged commit.** Fixing the workflow means a new
  tag, never a re-run.

## Step 6 — watch, then verify

```bash
gh run watch --exit-status $(gh run list --workflow deploy-production.yml --limit 1 --json databaseId --jq '.[0].databaseId')
```

**Budget ~15 minutes and do not promise a faster next one.** `cdk diff` builds
all three Lambdas during synth from a cold Rust cache on _every_ tag — the
cache is written under the tag's own ref and no tag can read another's. The
diff is ~11 min of that; everything after it is seconds. Full reasoning in
`docs/deployment.md` § Releases.

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
