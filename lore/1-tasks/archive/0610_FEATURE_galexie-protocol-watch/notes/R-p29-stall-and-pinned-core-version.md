# The P29 stall, the lead time, and reading the pinned core version (2026-10-02)

Moved out of the task README when it was activated.

**The P29 stall (UTC).** Pubnet voted protocol 29 at 2026-10-01 17:00:07,
ledger 64,717,645. Galexie 28.0.1 exported up to 64,717,644 and then logged
`History: Skipping catchup: incompatible core version or invalid local state`.
`production-galexie-ingestion-lag` fired at 17:08:56; 29.0.0 was deployed at
06:27 the next morning (0605); the first P29 export landed at 07:04. Both the
explorer and stellar-prices-api went ~14 h without new ledgers, plus ~2.5 h
of catch-up. The ECS healthcheck (`pgrep -x stellar-core`) stayed green.

**The lead time this check would have had:**

- Horizon reported `core_supported_protocol_version = 29` from about
  2026-09-22 (stellar-prices-api's XDR watch opened its issue #336 that day).
- A Galexie image with captive core 29 existed from 2026-09-24: Docker Hub
  commit tag `c927ffc` has the same digest (`sha256:538ad0fb…`) as today's
  `29.0.0`, and its config label `org.opencontainers.image.created` is
  `2026-09-24T16:53:32Z`. The `29.0.0` tag itself was last pushed
  2026-10-01 20:38, after the vote.
- So: red from 09-22, actionable from 09-24 — about a week before the vote.

**Reading the pinned version — tags do not work, the image config does.**
`infra/envs/production.json → galexieImageTag` is the manifest-list digest
`sha256:5269dfd9…`. Docker Hub's tag list no longer maps any tag to it: `29.0.0`
was re-pushed as the single-platform manifest `sha256:538ad0fb…`. A
digest-to-tag lookup therefore fails on day one. What works, anonymously:

1. token: `https://auth.docker.io/token?service=registry.docker.io&scope=repository:stellar/stellar-galexie:pull`
2. `GET https://registry-1.docker.io/v2/stellar/stellar-galexie/manifests/<pinned digest>`
   with the manifest-list and image-manifest `Accept` types → the linux/amd64
   platform manifest
3. that manifest's config blob → `Env` →
   `STELLAR_CORE_VERSION=29.0.0-3589.4eb833373.noble` → major 29

Do not use `org.opencontainers.image.version`: it is `24.04`, the Ubuntu base.
If the pinned digest is not on Docker Hub (a self-built image) or carries no
`STELLAR_CORE_VERSION`, the check must fail as "cannot determine", never pass.

**Notification.** Slack is closed for GitHub workflows here, as recorded in
stellar-prices-api's `deploy-ledger-processor.md` ("Why not Slack"): the
workspace is at its installed-app limit, the Slack GitHub app needs an org
owner, and SNS → Chatbot needs AWS credentials. Oskar settled on issue +
email for the prices watch. Note that GitHub's scheduled-workflow failure
email reaches only whoever last edited the `cron:` line.
