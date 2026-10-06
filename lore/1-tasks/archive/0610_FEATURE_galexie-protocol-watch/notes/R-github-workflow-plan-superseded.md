# The GitHub-workflow plan, superseded 2026-10-05

The plan the task was filed with. Superseded by the Lambda in the README:
the pin in `production.json` is the ECR digest, and ECR rewrites the
manifest of a multi-arch image, so for 26.1.0 and 27.0.0 that digest is not
on Docker Hub (404, checked 2026-10-05; 28.0.1 and 29.0.0 are). A workflow
reading Hub by the pin would have reported "cannot determine" after every
such bump, and reading ECR from GitHub needs a new AWS role.

## The plan as filed

### Step 1: The check (v1)

A dependency-free script plus a scheduled workflow (daily + `workflow_dispatch`),
modelled on stellar-prices-api's `verify-xdr-protocol-gap.mjs` and
`xdr-protocol-watch.yml`. Horizon `current_protocol_version` and
`core_supported_protocol_version` against the captive-core major read as above:

| Tier     | Condition                                 | Run   | Issue                                   |
| -------- | ----------------------------------------- | ----- | --------------------------------------- |
| OK       | core major ≥ core supports                | green | an open one is closed                   |
| LAGGING  | core major < core supports, not yet voted | red   | opened: "bump Galexie before the vote"  |
| BEHIND   | core major < mainnet current              | red   | opened, or LAGGING escalated by comment |
| NO CHECK | Horizon or the registry unreadable        | red   | untouched                               |

LAGGING should say whether Docker Hub already has an image whose
`STELLAR_CORE_VERSION` reaches the new protocol, so the issue tells the reader
if the bump is possible today.

### Step 2: Delivery

One tracking issue, opened once, body refreshed silently, a comment only on
LAGGING → BEHIND, closed on OK (the prices watch's lifecycle). Assign it, or
@-mention the Galexie owners in it, so the notification reaches people and not
only the cron editor's inbox.

### Step 3: Branches

The default branch is `master`, and a `schedule` trigger only runs the
default branch's copy. Put the workflow on `master` and let it check out
`develop` for the script and `production.json`, as the prices watch does.
GitHub starts these crons 5–8 h late (the prices watch: cron 06:17, actual
starts 11:25–13:56 UTC) — fine for a week of lead time.

### Step 4 (optional, v2): read production, notify Slack

A scheduled Lambda beside the existing probes reads the running
`production-galexie-live` task definition's image instead of the pinned
digest (production, not intent: on 2026-10-02 29.0.0 was deployed at 06:27
and PR #590 put it on `develop` at 07:12), publishes a metric, and an alarm goes through the existing
SNS → AWS Chatbot → Slack path (the Chatbot Slack workspace is already
authorized). A CDK change, a deploy and a `docs/architecture/**` update.
