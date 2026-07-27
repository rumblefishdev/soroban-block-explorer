---
id: '0302'
title: 'FEATURE: Drop both AWS WAF WebACLs and reconcile every doc that claims them'
type: FEATURE
status: completed
related_adr: ['0048', '0032']
related_tasks: ['0277', '0312']
tags:
  [
    effort-medium,
    priority-high,
    security,
    waf,
    cloudflare,
    cost,
    docs,
    infra,
    scf,
  ]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: fmazur
    note: 'Spawned from 0277 future work.'
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Renumbered 0283 → 0302 to resolve a shared-ID collision — '0283' was also
      claimed (same day) by the active contract-type-rebuild task (older,
      milestone-1, with child tasks 0294-0297, and the namesake of branch
      fix/0283). Content unchanged; only id + filename. Heads-up @fmazur.
  - date: 2026-07-27
    status: active
    who: karolkow
    note: >
      Activated and widened. The original scope (one config flag) was incomplete
      in three ways, each verified against the tree and against live AWS:
      (1) removing the WAF from the CDK app — whether by flag or by deleting the
      code, as was ultimately decided — does not delete the deployed us-east-1
      stack, and once it is out of the app `cdk destroy` can no longer address it,
      so the teardown needs a raw CloudFormation delete;
      (2) a full-tree sweep found AWS WAF asserted across 17 files outside the
      milestone-3 package — architecture docs, operational guides, load-test
      material, code comments, config and four records with stale pointers — plus
      14 hits in the milestone-3 evidence package, which is reconciled
      separately (Out of scope) because those files carry uncommitted edits
      elsewhere; (3) the CDK app takes its target account from ambient
      credentials, and more than one account is reachable from an operator laptop.
      Cost-confirmation dropped as an acceptance criterion — the WebACLs should
      not have outlived this task's creation, so a month-later cost check is not
      a gate.
  - date: 2026-07-27
    status: active
    who: karolkow
    note: >
      Teardown executed and verified. Two `--exclusively` deploys (Delivery 79 s,
      ApiGateway 23 s, neither prompting for approval since every IAM change was a
      removal) followed by a raw `delete-stack` of the orphaned us-east-1 stack.
      Both WebACLs gone, us-east-1 holds only `CDKToolkit`, `/cdk/exports/` empty,
      stage throttle intact at 50.0/100, frontend 200, `/v1/ledgers` 401,
      `/api-docs-json` 200. Scope grew during the task: the WAF construct code was
      deleted outright rather than left behind `enableWaf:false`, which also
      removed the app's only cross-region reference. 14 of 15 acceptance criteria
      met; the outstanding one is the `docs/scf/` handover, deliberately out of
      scope. Unrelated `CloudflareBootstrap` drift re-measured and appended to
      the existing 0312.
  - date: 2026-07-27
    status: completed
    who: karolkow
    note: >
      Closed. Merged to master as PR #362 (5 commits, 27 files, +883/-500 on the
      code and docs, plus review fixes). 14 of 15 acceptance criteria met; the
      fifteenth — reconciling the 14 AWS WAF claims in `docs/scf/**` — was
      deliberately out of scope and is handed to whoever owns that package's
      pending edits. It must land before milestone 3 is submitted; this task is
      not self-sufficient for the submission. A review after the fact raised two
      operational follow-ups. Neither reverses the decision and neither is
      recorded here: they are held outside the repository, which is public.
---

# Drop both AWS WAF WebACLs and reconcile every doc that claims them

## Summary

Remove AWS WAF from the system: delete the constructs and the `enableWaf` /
`*WafRateLimit` settings from the CDK app, deploy so both WebACLs go, delete the
orphaned us-east-1 stack, and bring the architecture and operational docs that
assert AWS WAF as an active security control back in line with reality.

The infra change and those doc changes ship together — otherwise the docs describe
a control that no longer exists. The milestone-3 evidence package is reconciled
separately; see Out of scope.

## Context

[ADR 0048](../../../2-adrs/0048_cloudflare-edge-over-aws-waf.md) Decision 5 accepted
the teardown on 2026-06-10 and deferred it pending a soak. The soak has since run
in production for seven weeks with no regression attributable to the Cloudflare
edge.

**State before the teardown**, verified in account `750702271865` on 2026-07-27
with read-only calls. Kept as the starting point this task measured against; for
what it looks like now see Teardown outcome.

| Fact                                   | Value                                                                                                                  |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| REGIONAL WebACL                        | `production-soroban-explorer-api`, id `53f5bd6b-1b8d-4f64-9449-1efa6b3c2fb6`                                           |
| …attached to                           | API Gateway stage `production` on REST API `6l9k06w4pl`                                                                |
| CLOUDFRONT WebACL                      | `production-soroban-explorer-cf`, id `33cd763f-05d1-4fe0-935a-494d6a3c7d00` (us-east-1)                                |
| …attached to                           | distribution `EA2TLS5SS5M87` (`sorobanscan.rumblefish.dev`) — the only distribution in the account                     |
| Stack to delete                        | `Explorer-production-CloudFrontWaf` (us-east-1), last updated 2026-07-17T13:38:56Z; only Explorer stack in that region |
| Throttling (independent of WAF, stays) | stage `*/*` — `throttlingRateLimit 50.0`, `throttlingBurstLimit 100`                                                   |
| AWS WAF spend                          | May 3.21 USD · Jun 18.07 USD · Jul-to-date 15.49 USD (estimated)                                                       |

The Cloudflare migration covered half the edge. The frontend is **not** behind
Cloudflare: `sorobanscan.rumblefish.dev` resolves to CloudFront directly (Route 53
delegation, no Cloudflare nameserver anywhere in the chain, no `cf-ray` in the
response headers). The Turnstile checkbox visible on the page is a widget loaded
by our own bundle to obtain an API session — it is not Cloudflare filtering
traffic to the domain, and mistaking one for the other is the easy error here.
ADR 0048's acceptance note records the cause: the nameserver flip happened on the
`rumblefishdev.com` registrar, not on the parent `rumblefish.dev` zone.

So the teardown is two unlike operations, not one:

|                       | Duplication with Cloudflare                   | Deployment risk                                                           |
| --------------------- | --------------------------------------------- | ------------------------------------------------------------------------- |
| REGIONAL (API) WebACL | real — Cloudflare does the same work          | none: one stack, one region                                               |
| CLOUDFRONT WebACL     | none — it is the frontend's only edge control | the cross-region stack destroy, the most failure-prone step in the change |

**Decision taken 2026-07-27: drop both.** The frontend is to be left with no edge
control, deliberately. Rationale on the merits, not on cost: the SPA is a static
React bundle on CloudFront, edge-cached, origin a private S3 bucket via Origin
Access Control. The injection-oriented managed rules (SQLi, XSS, bad inputs, IP
reputation) have nothing to protect there — there is no application, only files.
Exactly one rule carried value, the 10 000-per-5-minute per-IP cap, and what it
guarded was transfer cost under scraping, not integrity. AWS Shield Standard stays
either way (volumetric L3/L4; it does not cap HTTP requests per IP).

That also inverts the "gap" framing: **the frontend having no Cloudflare in front
of it may be the correct end state rather than a backlog item** — putting
Cloudflare ahead of CloudFront stacks two CDNs on each other. Treat ADR 0048's
"move the frontend zone first" path as an open question, not a deferred
obligation.

`enableOriginSecretLock: true` is **not** a substitute: it rejects every request
without an `X-Origin-Secret` header that only a Cloudflare Transform Rule injects,
so with the frontend outside Cloudflare it takes the site down for every visitor.

## Implementation

### 1. Code and config — remove, do not disable

Decision taken 2026-07-27: the WAF code goes with the WebACLs rather than being
left behind a flag. Nothing is coming back — the frontend is deliberately without
edge protection and Cloudflare covers the API — so a dormant `enableWaf` switch
would be dead flexibility with a live footgun attached.

- `infra/src/lib/constructs/waf-web-acl.ts` and
  `infra/src/lib/stacks/cloudfront-waf-stack.ts` — deleted.
- `app.ts` — the `CloudFrontWafStack` instantiation, `cloudFrontWafArn`, and the
  `crossRegionReferences: true` it required. This removes the **only** cross-region
  reference in the app.
- `delivery-stack.ts` — the `cloudFrontWafArn` prop, the `webAclId` spread, the
  conditional output.
- `api-gateway-stack.ts` — the `WafWebAcl` instantiation and its
  `CfnWebACLAssociation`. Watch for the trailing `if (waf)` output block further
  down the file: removing only the declaration leaves a dangling reference and
  `tsc` fails with `TS2304: Cannot find name 'waf'`.
- `types.ts` — `enableWaf`, `cloudFrontWafRateLimit`, `apiWafRateLimit`, their two
  `< 100` validations, and the `enableWaf` term in the soft edge-gating warning.
- `infra/envs/production.json` — the same three keys.

The resulting CloudFormation template is identical to what `enableWaf: false`
would have produced, so the deploy diff is the same either way.

### 2. Teardown, in this order

The us-east-1 stack **cannot** be deleted with CDK. `cdk destroy` resolves stack
names from the synthesized app; with the code gone `CloudFrontWafStack` is not in
the app, so CDK reports no matching stack while the real one keeps existing and
billing. `cdk ls` confirms it is absent. There is also no
`destroy-production-cloudfront-waf` target in `infra/Makefile`.

1. Assert the target account before anything else. `infra/src/lib/app.ts` sets
   `account: process.env['CDK_DEFAULT_ACCOUNT']` — nothing pins production, and
   an operator laptop can reach more than one account. Production is
   `750702271865`; confirm with `aws sts get-caller-identity`.
2. `make -C infra diff-production` and read it in full.
3. Deploy so that `DeliveryStack` no longer references the cross-region ARN and
   `ApiGatewayStack` no longer creates the REGIONAL WebACL or its association.
4. Confirm the distribution carries no `WebACLId`. This gate is load-bearing, not
   cosmetic, and the mechanism is measured, not assumed. The us-east-1 stack holds
   `ExportsWritereucentral1E172851B74269898` (`Custom::CrossRegionExportWriter`);
   `DeliveryStack` holds `ExportsReader8B249524`
   (`Custom::CrossRegionExportReader`). The export parameter lives in
   **eu-central-1**, not us-east-1 (`/cdk/exports/` is empty there):

   ```
   /cdk/exports/Explorer-production-Delivery/ExplorerproductionCloudFrontWafuseast1FnGetAttWafWebAclBE24253CArnA83F22D5
     = arn:aws:wafv2:us-east-1:750702271865:global/webacl/production-soroban-explorer-cf/33cd763f-…
   ```

   On delete the writer refuses to remove a parameter a consumer still claims, and
   the whole stack delete fails with it. Redeploying `DeliveryStack` first is what
   releases the claim — verify that parameter is gone before step 5.

5. Delete the orphaned stack with raw CloudFormation:
   `aws cloudformation delete-stack --region us-east-1 --stack-name Explorer-production-CloudFrontWaf`
   That stack holds only the WebACL, its log group, `AWS::Logs::ResourcePolicy`,
   `AWS::WAFv2::LoggingConfiguration`, and the export writer — nothing shared.

Removal is one-way in practice: the WAF log groups were `RemovalPolicy.DESTROY` in
the construct this task deletes (see the `feat(lore-0302)` commit for its last
state), so re-arming means a fresh deploy and the old logs are gone. Every
production deploy is manual from a laptop; there is no CI deploy path for
production — see [`docs/deployment.md`](../../../../docs/deployment.md) § No
staging, no CI, which is the current authority. ADR 0001 still describes CI/CD
deployment because that was the intent when it was written; the dead workflow is
task 0390's scope, not this one.

### 3. Documentation and comments — 17 files

Everything that asserts AWS WAF as a live control is in scope **except**
`docs/scf/**` (see Out of scope). File-by-file, with what to do in each and why
the excluded ones are excluded:
[notes/G-waf-claim-inventory.md](notes/G-waf-claim-inventory.md).

17 files outside the milestone-3 package mention AWS WAF — 153 grep hits, but that
count is not the size of the job. 111 of them sit in three documents that are
records rather than descriptions (`docs/waf-vs-cloudflare/README.md` 41, ADR 0048
27, archived task 0277 43); those get a banner or a one-line pointer fix, not a
rewrite. Grouped:

| Group                                                                                                                              | Files | Hits |
| ---------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| `docs/architecture/**` — obligatory in the same PR per [ADR 0032](../../../2-adrs/0032_docs-architecture-evergreen-maintenance.md) | 4     | 18   |
| Operational guides — `docs/deployment.md`, `docs/waf-vs-cloudflare/README.md`, `infra/README.md`                                   | 3     | 48   |
| Load-test material — harness `README` + `main.rs` (annotate, do not rewrite)                                                       | 2     | 7    |
| Present-tense code comments — `crates/api` request-id + config, `compute-stack.ts`                                                 | 3     | 3    |
| `infra/envs/production.json`                                                                                                       | 1     | 3    |
| Records needing a pointer or history fix only — ADR 0048, ADR 0001, task 0277, backlog 0090                                        | 4     | 74   |

Re-grep before editing and reference sections, not line numbers.

**The editing rule — current state gets rewritten, history gets a note.** A
sentence that describes how the system works today is corrected. A sentence that
records something that happened — a measurement taken against the WAF, why a
harness was configured a certain way, a comparison made to reach a decision — is
**left standing** and gets a dated note or banner instead. Deleting it would
assert that the run never happened under those conditions, which is a different
falsehood from the one this task is fixing. The load-test material is the clearest
case: the tiers really were driven with the per-IP rule lifted, and that stays
written down.

Two judgement calls follow from that rule and are recorded in the note so they
read as decisions rather than oversights: `docs/waf-vs-cloudflare/README.md` gets
an outcome banner rather than 41 rewrites (a dated comparison written to reach a
decision, not a description of the system), and the load-test material is
annotated rather than rewritten, because it records the conditions real
measurements were taken under.

**Do not repeat ADR 0048's cost framing uncritically.** Its primary rationale was
that AWS WAF's `$0.60/M` scales with abuse traffic. At ~130k req/mo that
component is ~0.08 USD. The saving is real, but it is the fixed WebACL + rule
fee, not runaway metered cost.

## Out of scope

- **The milestone-3 evidence package under `docs/scf/`.** 14 WAF claims live there
  (evidence document, security checklist, form answers, video scenario, and the
  `.mmd` that renders Figure 1) and all of them go false with this teardown, but
  they are not edited here. Those files carry uncommitted corrections in a second
  worktree — evidence ±124 lines, video ±31, form-answers ±2, PDF rebuilt —
  unrelated to WAF: measured request count, a raised enrichment-DLQ alarm, the
  Scene 4 dashboard claim. Editing the same regions from a second branch would
  conflict with or drop that work. The package is reconciled separately, after
  this task lands, by whoever owns those pending edits; that includes rebuilding
  the tracked PDF (`docs/scf/build-pdf.sh 3`) and re-dating the checklist sign-off
  (currently 2026-07-25). **This task is therefore not self-sufficient for the
  submission** — both halves must land before milestone 3 is submitted.
- Moving the frontend zone behind Cloudflare. Blocked on parent-zone-owner
  sign-off since June; that is the proper completion of ADR 0048 and needs its
  own task once the delegation is available.
- Performing a real restore from the Hetzner Storage Box. The architecture doc's
  "verified restorable" claim was narrowed to what `docs/backups.md` actually
  supports (locally drill-tested procedure); exercising the off-box path is
  separate work.
- The CloudWatch dashboard needs no change: it has no WAF widget and no WAF alarm
  (`infra/src/lib/stacks/cloudwatch-stack.ts` has zero WAF references).
- Third-party WAF mentions stay: `docs/runbooks/…fresh_machine.md` describes
  Cloudflare's WAF on `mainnet.sorobanrpc.com`, and `infra/cloudflare/**`
  describes the Cloudflare zone's own rulesets. Both remain true.
- Archived lore tasks keep their WAF text — they record work already done.
  Rewriting them would falsify the record. Only 0277's forward pointer is fixed.

## Acceptance Criteria

- [x] WAF constructs, stack, gates and the three config keys removed; `tsc` green
      and `cdk ls` no longer lists `Explorer-production-CloudFrontWaf`
- [x] Target account asserted as `750702271865` before any deploy
- [x] REGIONAL WebACL gone; API Gateway stage `webAclArn` is null
- [x] CLOUDFRONT WebACL gone; distribution `EA2TLS5SS5M87` carries no `WebACLId`
- [x] `Explorer-production-CloudFrontWaf` deleted; us-east-1 holds no Explorer
      stack
- [x] Throttling survived — stage `*/*` still 50.0 / 100 (claimed independently
      as security-checklist control 3)
- [x] Frontend 200; `/v1/ledgers` still 401; `/api-docs-json` still 200
- [x] All four `docs/architecture/**` files reconciled (ADR 0032)
- [x] `docs/deployment.md` documents the delete path; `infra/README.md` records
      the flag state — as it turned out, by dropping the stack row entirely, since
      the stack no longer exists in the app
- [x] `docs/waf-vs-cloudflare/README.md` carries an outcome banner and no longer
      asserts the toggle as live
- [x] Present-tense code comments corrected: `crates/api/src/common/request_id.rs`,
      `crates/api/src/config.rs`, `infra/src/lib/stacks/compute-stack.ts`
- [x] Load-test material **annotated, not rewritten** —
      `crates/load-tests/README.md` and `src/main.rs` keep the record of what the
      runs were driven against; only the forward-looking instruction is restated
- [x] Stale pointers fixed: ADR 0048, ADR 0001, task 0277, backlog 0090
- [x] No historical statement anywhere was rewritten into the present tense (the
      editing rule in § 3)
- [ ] `docs/scf/` handed over for separate reconciliation (out of scope here, but
      must land before milestone 3 is submitted)

## Teardown outcome — measured 2026-07-27, account `750702271865`

Two deploys, both `--exclusively`, then a raw CloudFormation delete. No approval
prompt on either deploy: every IAM change was a removal.

| Step                                                         | Result                                                                                                                       |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `deploy Explorer-production-Delivery --exclusively`          | 79 s. Distribution `WebACLId` → `""`; `ExportsReader` + its role and handler destroyed; `CloudFrontWafWebAclArn` output gone |
| `deploy Explorer-production-ApiGateway --exclusively`        | 23 s. WebACL, association, logging config, log group and log resource policy destroyed; `ApiWafWebAclArn` output gone        |
| `delete-stack Explorer-production-CloudFrontWaf` (us-east-1) | Clean. us-east-1 now holds `CDKToolkit` only                                                                                 |

Post-state, all read-only:

- `wafv2 list-web-acls` REGIONAL/eu-central-1 → empty; CLOUDFRONT/us-east-1 → empty
- API Gateway stage `webAclArn` → `null`
- Stage throttle → `rate 50.0`, `burst 100` — survived, as required
- `/cdk/exports/` in eu-central-1 → empty
- Frontend `200`, `/v1/ledgers` `401`, `/api-docs-json` `200`

The export-claim gate behaved exactly as predicted: after the `Delivery` deploy the
SSM parameter still existed but `list-tags-for-resource` returned `TagList: []`, so
the consumer marker was released and the stack delete went through unblocked.

## Design Decisions

### From Plan

1. **Both WebACLs, not just the API one.** The frontend is left with no edge
   filtering, deliberately — see Context.
2. **Deploy only the two affected stacks, `--exclusively` on `ApiGateway`.**
   `make deploy-production` is `--all` and production carries undeployed drift in
   `Compute` (three Rust Lambda assets, two secret descriptions) and in
   `CloudflareBootstrap`. `Delivery` has no dependencies; `ApiGateway`
   `addDependency(compute)`, so without `--exclusively` it would have shipped
   unrelated Rust code.
3. **Docs edited by the history rule** — current state rewritten, records annotated.

### Emerged

4. **Delete the WAF code outright rather than leave it behind `enableWaf:false`.**
   Decided mid-task. The re-arm argument had already collapsed (the frontend is
   deliberately unprotected and fronting CloudFront with Cloudflare would stack two
   CDNs), leaving a dormant flag whose only effect would be to recreate a
   cross-region stack that is awkward to delete. The resulting CloudFormation
   template is identical either way, so the deploy risk did not change.
5. **`validateConfig`'s edge-gating warning kept, reworded.** With `enableWaf` gone
   the condition reduces to `!enableBasicAuth && !enableOriginSecretLock`, which is
   permanently true on production. Downgraded from WARNING to NOTE and reworded to
   say the state is intentional, so it stays informative instead of becoming noise
   that operators learn to ignore.
6. **Template snapshots taken before each deploy** (`get-template --template-stage
Original`) as a rollback path independent of the git working tree. Not needed.
7. **Drift found in `CloudflareBootstrap` appended to the existing
   [0312](../../backlog/0312_OPS_cloudflare-bootstrap-orphan-dead-origin-secret.md)**
   rather than folded in here — unrelated to WAF, and it would have shipped
   silently under `--all`. It was first spawned as a new task before a backlog
   search turned up 0312, which had recorded the same `orphan` diff five weeks
   earlier and had already answered the open question (nothing reads it; the live
   edge-auth secret is a different resource in `Compute`). The duplicate was
   withdrawn. Search the backlog before spawning — this repo has a documented
   history of id collisions.

## Issues Encountered

- **Partially-removed WAF code broke the build.** The construct removal deleted the
  `const waf = …` declaration in `api-gateway-stack.ts` but left the matching
  `if (waf) { new cdk.CfnOutput(…) }` block ~150 lines further down, so `tsc`
  failed with `TS2304: Cannot find name 'waf'` twice. Not a regression — an
  unfinished edit. Worth knowing that the WAF surface in that file is in two places,
  not one.
- **`cdk diff` hides changes containing non-ASCII characters** ("Omitted N changes
  because they are likely mangled non-ASCII characters"). Re-running with `--strict`
  showed the omitted entries were `CDK::Metadata` analytics blobs plus two secret
  descriptions in `Compute`. Harmless here, but a diff read without `--strict` is
  not a complete diff.
- **An earlier audit of this work judged file:line references against the wrong
  baseline.** The handover being verified had been written in a second worktree with
  ~124 lines of uncommitted edits in the same files, so line numbers that looked
  stale were correct there. Check `git worktree list` plus each tree's status before
  calling a reference wrong.

## Future Work

- _(The construct-code question was decided during the task rather than deferred:
  delete. See § 1.)_
- Two operational follow-ups from the post-teardown review are held outside
  this repository. They concern where abusive traffic is absorbed and what
  notices it — not a defect in the teardown, and not something to restate in
  a public repo. Ask the task owner for the handover.

## Branch

Branched from `master`, not `develop`: the milestone-3 work this teardown
invalidates exists only on `master`. `master` is merged into `develop` once the
milestone-3 stream is finished, which is what resynchronises the two lines —
including this task's `backlog/` → `active/` move.
