---
id: '0306'
title: 'FEATURE: SNS fan-out on stellar-ledger-data (S3 → SNS → indexer queue + prices-api queue)'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: ['infra', 'cdk', 'cross-team', 'phase-launch']
links:
  - '.temp/G-be-sns-fanout-handoff.md'
history:
  - date: 2026-06-19
    status: active
    who: fmazur
    note: 'Task created from cross-team handoff (prices-api team). Implements BE side of the SNS fan-out contract.'
  - date: 2026-06-22
    status: active
    who: fmazur
    note: >
      Deployed Explorer-production-Compute to prod (cdk deploy, IAM-approved).
      SNS fan-out live: S3→SNS→SQS, rawMessageDelivery=true, 5× /platform/
      SSM keys. Verified prod: indexer still paused (concurrency 0, no ESM),
      doorbells still landing in ledger-ingest via SNS (~103.6k backlog,
      NotVisible 0). All BE acceptance criteria met except joint verification
      with prices-api (their subscribe side). Stays active until that closes.
  - date: 2026-06-22
    status: completed
    who: fmazur
    note: >
      BE side complete and shipped. Closing per operator: the BE deliverable
      (topic + S3→SNS cutover + rawMessageDelivery subscription + 5× /platform
      SSM keys) is deployed and verified on prod. The one remaining item —
      joint verification with prices-api — depends on the other team's subscribe
      side and is handled cross-team off-board (not tracked as a separate task,
      per operator). One file changed (compute-stack.ts) + 2 architecture docs;
      /code-review max corrected a false rawMessageDelivery invariant before merge.
---

# FEATURE: SNS fan-out on `stellar-ledger-data`

## Summary

Insert an SNS topic (`{env}-ledger-events`) between the `stellar-ledger-data`
S3 bucket and the indexer's `ingestQueue`, so a second tenant (the prices-api
project, **same AWS account**) can subscribe its own queue to the same
`ObjectCreated` (`.xdr.zst`) doorbells. S3 permits only one destination per
overlapping `event + suffix`, so the direct `S3 → SQS` wiring is **replaced**
by `S3 → SNS`, with the indexer's queue re-subscribed via SNS using
`rawMessageDelivery: true` (keeps the SQS body byte-identical to the legacy
shape; note the indexer reads only `messageId` and ignores the body, so this is
not load-bearing for us — see Issues Encountered). Driven by the cross-team
handoff `.temp/G-be-sns-fanout-handoff.md`.

## Status: Completed

**Current state:** **DEPLOYED to production and verified** (2026-06-22). The
cutover was done directly on prod _while the indexer is paused_
(`indexerLambdaConcurrency = 0`) — the safest window, since there is no live
consumer to disrupt and the reconcile-from-S3 backstop covers any doorbell lost
during the near-atomic notification swap. Post-deploy verification all green
(see Verification below). **Remaining:** joint verification with prices-api
(their subscribe side) — then this task can close.

### Verification (prod, 2026-06-22)

- Indexer paused: `ReservedConcurrentExecutions = 0`, `list-event-source-mappings
= []` → nothing drains the queue.
- Doorbells still flow `S3 → SNS → SQS`: `ledger-ingest`
  `ApproximateNumberOfMessages` rising (103,588 → 103,599 across reads),
  `NotVisible = 0`. (NB: ~103.6k backlog accumulated during the pause; drains
  via reconcile when the indexer is eventually resumed — content-free doorbells,
  state from CH `max(sequence)` + S3, so no ledger lost even if old doorbells
  expire at the 14-day retention.)
- S3 notification swapped: `TopicConfigurations` → `production-ledger-events`,
  suffix `.xdr.zst`, no `QueueConfigurations`.
- Subscription `RawMessageDelivery = true`, endpoint = `ledger-ingest` queue.
- 5× `/platform/production/*` SSM params present (ch-domain =
  `ch.sorobanscan.rumblefish.dev`, topic ARN, bucket name/arn, network
  passphrase).

## Context

The prices-api team (separate project, **shared AWS account** — confirmed by
operator on 2026-06-19) needs the same ledger `ObjectCreated` events we already
consume. Their subscriber-side CDK is authored and prepare-only (their PR #34,
their task IDs — **not** ours) and gated on us landing the topic + SSM keys.

Handoff claims were verified against `infra/src/lib/stacks/compute-stack.ts`
on `develop`:

- Live wiring is `ledgerBucket.addEventNotification(OBJECT_CREATED,
new s3n.SqsDestination(ingestQueue), { suffix: '.xdr.zst' })` at L386–390.
- `ledgerBucketName` / `ledgerBucketArn` are stack props (L20–21).
- `rawMessageDelivery: true` is set, but it is **not** load-bearing for our
  indexer: `SqsMessage` (`crates/indexer/src/handler/mod.rs:56–61`) reads only
  `messageId` and ignores the body. The handoff's claim that the parser "breaks
  on every ledger" without it is false for us (it matters for prices-api, which
  reads the S3 key from the body). See Issues Encountered.

**Same-account premise confirmed** → no cross-account topic policy needed;
prices-api subscribes its own queue via its own deploy-role IAM. If that ever
changes, this task's scope expands to include a cross-account SNS topic policy.

## Implementation Plan

(Per handoff `.temp/G-be-sns-fanout-handoff.md`, Steps 1–7. All in
`infra/src/lib/stacks/compute-stack.ts` unless noted.)

### Step 1 — Imports

Add `sns`, `sns-subscriptions` (`subs`), `ssm` imports.

### Step 2 — Topic

`new sns.Topic(this, 'LedgerEventsTopic', { topicName: `${config.envName}-ledger-events` })`,
placed just above the bucket-notification block.

### Step 3 — Repoint S3 notification (S3 → SNS) — REPLACE

Swap `s3n.SqsDestination(ingestQueue)` → `s3n.SnsDestination(ledgerEventsTopic)`,
same `{ suffix: '.xdr.zst' }`. CDK auto-adds the topic policy allowing S3 to
publish. **This is a replace of the live notification, not an add.**

### Step 4 — Re-subscribe indexer queue (⚠️ rawMessageDelivery)

`ledgerEventsTopic.addSubscription(new subs.SqsSubscription(ingestQueue, { rawMessageDelivery: true }))`.
Leave the existing `SqsEventSource(ingestQueue, …)` (L398) and
`ingestQueue.grantConsumeMessages(processorFunction)` (L411) untouched.

### Step 5 — Publish topic ARN to SSM

`/platform/${config.envName}/ledger-events-topic-arn` (net-new namespace; we
currently publish under `/soroban-explorer/{env}/*`). Read by prices-api CDK at
deploy time only.

### Step 6 — Publish remaining `/platform/{env}/*` keys

`stellar-ledger-data-bucket-name` (`props.ledgerBucketName`),
`stellar-ledger-data-bucket-arn` (`props.ledgerBucketArn`),
`ch-domain` (`config.chDomainName`),
`stellar-network-passphrase` (`config.stellarNetworkPassphrase`).
Decide namespace: handoff proposes `/platform/{env}/*`; confirm we want a
shared cross-team namespace vs reusing `/soroban-explorer/{env}/*`.

### Step 7 — Topic policy

Default CDK `sns.Topic` does not restrict subscribers; confirm no
BE-principal-only restriction is added. Same account → nothing to do.

### Cutover (no-drop path)

1. Deploy to a non-prod env first; confirm indexer keeps draining
   `ingestQueue` with no parser errors (proves rawMessageDelivery applied).
2. Deploy prod in a low-write window; the
   `PutBucketNotificationConfiguration` swap is near-atomic but is the single
   path that must not silently drop a ledger.
3. prices-api subscribes only after topic + SSM keys exist.

## Acceptance Criteria

- [x] `{env}-ledger-events` SNS topic created, one per env. (prod:
      `production-ledger-events`)
- [x] S3 `ObjectCreated` (`.xdr.zst`) publishes to the topic (replaces direct
      `S3 → SQS`). (verified: bucket `TopicConfigurations`, no
      `QueueConfigurations`)
- [x] `ingestQueue` re-subscribed with `rawMessageDelivery: true`; indexer ESM + parser unchanged; SQS body byte-identical to pre-change. (verified:
      `RawMessageDelivery = true`)
- [x] `/platform/{env}/*` SSM keys published (topic ARN, bucket name/arn,
      ch-domain, network-passphrase). (verified: 5 params under
      `/platform/production/`)
- [~] ~~Non-prod cutover verified~~ → **N/A: no non-prod env exists**
  (`production.json` is the only `EnvironmentConfig`). Cutover done on prod
  while the indexer was paused (`concurrency = 0`) — the safest window.
  "Indexer keeps processing" is moot (intentionally paused); instead verified
  doorbells keep landing in SQS via SNS (see Verification).
- [ ] Joint verification with prices-api: a new `.xdr.zst` PutObject delivers
      to both queues independently. **(external — prices-api's subscribe side;
      not a BE deliverable, not separately tracked)**
- [ ] **Docs updated** — `docs/architecture/**` ingestion-pipeline topology
      updated to show S3 → SNS → (indexer queue | prices queue) per
      [ADR 0032](../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — N/A — this task touches only
      `infra/**`, not `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Out of Scope

- Cross-account SNS topic policy (premise: same account; revisit only if that
  changes).
- prices-api subscriber-side CDK (their PR / their task).
- EventBridge alternative (weighed at the cross-team meeting; SNS chosen).

## Implementation Notes

All in `infra/src/lib/stacks/compute-stack.ts` (one file, as the handoff
predicted):

- Added imports: `sns`, `sns-subscriptions` (`subs`), `ssm`.
- `LedgerEventsTopic` (`{env}-ledger-events`) created above the bucket
  notification block.
- Bucket notification repointed `s3n.SqsDestination(ingestQueue)` →
  `s3n.SnsDestination(ledgerEventsTopic)`, same `{ suffix: '.xdr.zst' }`.
- `ingestQueue` re-subscribed via
  `ledgerEventsTopic.addSubscription(new subs.SqsSubscription(ingestQueue, { rawMessageDelivery: true }))`.
  The existing `SqsEventSource(ingestQueue, …)` and
  `grantConsumeMessages(processorFunction)` are untouched.
- Five `/platform/{env}/*` SSM params published via a loop over a
  `Record<string,string>` (topic ARN, bucket name, bucket arn, ch-domain,
  network passphrase).

Docs updated (ADR 0032):
`docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` (topology
diagram, doorbell description, §3.2 component list 6→7) and
`docs/architecture/technical-design-general-overview.md` (ASCII diagram arrow,
recovery paragraph, doorbell paragraph).

Verified locally only — `nx run …:lint` ✅ and `tsc --noEmit -p
tsconfig.lib.json` ✅. No `cdk synth`/`deploy` run (no AWS touched).

## Issues Encountered

- **Handoff's `rawMessageDelivery` rationale was wrong (caught by `/code-review
max`, CONFIRMED).** The handoff (and my first comment + doc edits) claimed
  that without raw delivery "SNS wraps the event in an envelope and the parser
  breaks on every ledger." False for our indexer: `SqsMessage`
  (`crates/indexer/src/handler/mod.rs:56–61`) deserializes only `messageId` and
  has no `body` field — the doorbell is content-free, so the envelope-vs-raw
  body shape is irrelevant to ingestion. Fixed the code comment and both
  architecture docs to state the accurate reason (raw delivery is for the
  prices-api consumer, which _does_ read the S3 key from the body). Not a code
  bug — the flag is still correctly set — but a misleading invariant that would
  have sent a future maintainer the wrong way during an incident.
- **Stale `.xdr.zstd` in the overview diagram (CONFIRMED).** Pre-existing doc
  bug re-exposed by my edit; the real S3 suffix filter is `.xdr.zst`. Fixed.
- **Recovery doc overstated durability (PLAUSIBLE).** My recovery edit implied
  `maxReceiveCount`/DLQ covers the whole tor; it covers only SQS → Lambda.
  Rewritten to name the no-DLQ SNS hops and the S3-reconcile backstop.

## Design Decisions

### From Plan

1. **SNS over EventBridge** — chosen at the cross-team meeting for latency;
   EventBridge's additive (no-touch to live `S3 → SQS`) advantage recorded but
   not chosen.
2. **`rawMessageDelivery: true`** — kept so the SQS body stays byte-identical to
   the legacy `S3 → SQS` shape. NB: **not** load-bearing for our indexer (see
   Issues Encountered) — it matters for the prices-api consumer, which reads the
   S3 key from the body.

### Emerged

3. **SSM namespace `/platform/{env}/*` (not our `/soroban-explorer/{env}/*`)** —
   chose the handoff's namespace because prices-api's CDK is _already authored_
   against `/platform/{env}/*` (their PR #34). Reusing our existing namespace
   would force them to rewrite their subscriber side; matching theirs is the
   lower cross-team-friction path. Trade-off: a new shared namespace now exists
   in our stack.
4. **SSM params emitted via a loop** over a `Record<string,string>` rather than
   five literal `new ssm.StringParameter(...)` blocks — less repetition; construct
   ids are `Platform-<key>`.
5. **`ch-domain` exposure to cross-tenant `/platform/*` accepted as-is** — the
   `/code-review max` pass (CONFIRMED finding) flagged that publishing the
   ClickHouse mTLS-proxy hostname to the shared namespace widens who can probe
   the CH endpoint, and that the fan-out itself only needs topic ARN + bucket.
   Operator decision (2026-06-19): **leave the key**. Rationale: same account;
   `ch-domain` is an endpoint behind mTLS, not a credential; and per ADR 0007
   (shared Hetzner CH sink) prices-api plausibly writes to the same CH and
   needs the host. Revisit if prices-api confirms it does _not_ need it.
6. **No DLQ on the new SNS → SQS hop; backstop is the S3 reconcile** — review
   (PLAUSIBLE) noted the deleted direct `S3 → SQS` was atomic, while the new
   `S3 → SNS → SQS` hop has no SNS-level redrive/DLQ, and `maxReceiveCount`
   covers only SQS → Lambda. Operator decision: **do not add a subscription
   DLQ** — SNS → SQS same-account delivery is extremely reliable (multi-day
   retry), and the durable backstop is unchanged: the file stays in S3 (ADR 0006) and the next doorbell's reconcile replays the contiguous gap forward
   (a single missed doorbell self-heals on the next ledger). Documented in
   `technical-design-general-overview.md` recovery section.

## Future Work

- Joint verification with prices-api (their subscribe side; both queues receive
  a single PutObject) — handled cross-team off-board, not tracked as a separate
  task per operator.
- If the `/platform/*` cross-team SSM contract grows beyond these five keys,
  consider an ADR documenting it.

## Notes

- Handoff numbering (`task 0050`, `PR #34`, `task 0038`) is the **prices-api
  project's** ID space, not ours — our 0038 is `cdk-environment-config`.
- Open decision for review: handoff publishes bucket-arn + ch-domain to a
  shared `/platform/*` SSM namespace, exposing those infra details to the
  other tenant. Network passphrase is public (mainnet), so non-sensitive.
- EventBridge was the lower-risk additive alternative (leaves S3 → SQS
  untouched); rejected at the meeting for latency. Recorded for traceability.
