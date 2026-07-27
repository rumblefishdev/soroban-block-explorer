---
type: generation
title: 'Every AWS WAF reference in the repo, classified'
status: mature
spawned_from: '0302'
spawns: []
---

# Every AWS WAF reference in the repo, classified

Full-tree sweep, 2026-07-27, case-insensitive `waf`, excluding `node_modules`,
`.trash/`, `infra/dist/`, `.git/` and XDR test fixtures (base64 corpora contain
the substring by chance — `scripts/enrichment-backfill-seed.sql:47` is the same
kind of false positive, a Stellar address).

**Reference sections, not line numbers.** Line numbers here are valid against
`master` at `067f72b7` only. The milestone-3 files additionally carry uncommitted
edits in a second worktree, which is why they are out of scope entirely.

## The rule this classification applies

A hit is **current state** if it describes how the system works now. Those get
rewritten.

A hit is **history** if it records something that happened — a measurement taken
while the WAF was active, why a harness was configured a given way, a comparison
made in order to reach a decision. Those are **left standing** and get a dated
banner or an appended note. Deleting them would assert that the run never happened
under those conditions — a different falsehood from the one this task exists to
fix, and a worse one, because the numbers stay while their explanation vanishes.

When a file mixes both — `crates/load-tests/README.md` is the clearest — split it
by sentence, not by file.

## In scope — 17 files, 153 grep hits

The hit count is a measurement, not a workload estimate: 111 hits sit in three
documents that record rather than describe (`docs/waf-vs-cloudflare/README.md` 41,
ADR 0048 27, archived task 0277 43), and those take a banner or a single pointer
fix. The files that actually assert a live control carry far fewer.

### Architecture docs (ADR 0032 obliges these in the same PR)

| File                                                          | Hits | What to do                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------------- | ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/architecture/infrastructure/infrastructure-overview.md` | 10   | Already forward-looking in three places ("Planned change (ADR 0048)… slated to be dropped"). Flip planned → done. Also `**AWS WAF**` as a live subsection, and two "should be protected by AWS WAF" lines.                                                                                                                                                                                                                                                                                                                                                                              |
| `docs/architecture/technical-design-general-overview.md`      | 5    | Describes the pre-migration edge and mentions Cloudflare **zero** times: an ASCII diagram box "REST, throttling, WAF", a technology table row, an "Edge Security \| AWS WAF" row, and a prose line pairing throttling with WAF. Plus an expanded, non-approved AC5 restatement that promised a "weekly Borg backup **verified restorable**". Narrowed: per `docs/backups.md` the restore procedure is drill-tested locally end-to-end, but a real restore from the Hetzner Storage Box has never been performed. "Verified restorable" claimed the second on the strength of the first. |
| `docs/architecture/backend/backend-overview.md`               | 2    | Ingress protection prose + a bullet "**AWS WAF** for managed-rule abuse protection on public ingress".                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `docs/architecture/frontend/frontend-overview.md`             | 1    | "API protection belongs at the API Gateway/WAF boundary, not in the bundle" — the boundary is now Cloudflare + the edge-secret lock.                                                                                                                                                                                                                                                                                                                                                                                                                                                    |

### Operational guides

| File                               | Hits | What to do                                                                                                                                                                                                                                                                                                                                                                                                  |
| ---------------------------------- | ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/deployment.md`               | 6    | Stack table row `CloudFrontWaf … (no target — see gotcha)` and the matching gotcha document how to **deploy** it; nothing documents how to **delete** it. Add the raw-CloudFormation step and record that production now runs `enableWaf: false`.                                                                                                                                                           |
| `docs/waf-vs-cloudflare/README.md` | 41   | A point-in-time comparison dated 2026-06-01, written to make the decision — not a description of the system. Do **not** rewrite 41 mentions: add an outcome banner at the top (decision executed, both WebACLs dropped, ADR 0048 + task 0302), retitle "## Current setup (AWS WAF)" to mark it historical, and fix the one live assertion — "Toggled by `enableWaf: true` in `infra/envs/production.json`". |
| `infra/README.md`                  | 1    | Stack list line `CloudFrontWafStack  CloudFront WebACL (conditional on enableWaf)`. Still literally true; note the flag is false in production so the stack is not created.                                                                                                                                                                                                                                 |

### Load-test material — annotate, do not rewrite

This is the sharpest case of the history rule. The tiers really were driven with
the per-IP WAF rule lifted; that is why the numbers are what they are. Removing the
WAF from this text would assert the runs happened under conditions they did not.

| File                            | Hits | What to do                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------- | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `crates/load-tests/README.md`   | 6    | Mixed. **Keep** the tier table's "116 / 1,157 / 5,787 per 5-min window vs the 2,000 per-IP WAF rule" reasoning and the paragraph explaining why the rule had to come off — those record the conditions of runs already made. **Add a dated banner** stating the WebACLs were dropped, so a future reader does not go looking for a rule that no longer exists. Only the forward-looking instruction — that a 50M tier "needs `loadTesting: true`" — needs restating against what actually limits it now (the 50 rps / 100 burst API Gateway throttle). |
| `crates/load-tests/src/main.rs` | 1    | Comment explaining that `reqwest` sends no User-Agent and AWS WAF's `CommonRuleSet` rejects that. **Keep setting the UA** and keep the comment; append that the AWS rule is gone and the equivalent now lives in Cloudflare's managed ruleset. Do not delete the original reason — it explains why the line exists.                                                                                                                                                                                                                                    |

### Code comments describing the current mechanism — rewrite

All three say the `loadTesting` switch lifts "the API Gateway throttle/WAF". After
the teardown it lifts the throttle only. Present tense, so they are corrected.

| File                                    | Hits | What to do                                                                       |
| --------------------------------------- | ---- | -------------------------------------------------------------------------------- |
| `crates/api/src/common/request_id.rs`   | 1    | "`loadTesting` flag that lifts the API Gateway throttle/WAF" → throttle only.    |
| `crates/api/src/config.rs`              | 1    | Same sentence, same fix.                                                         |
| `infra/src/lib/stacks/compute-stack.ts` | 1    | "throttle/WAF (api-gateway-stack.ts), so one switch sets the whole…" → same fix. |

### Config

| File                         | Hits | What to do                                                                                                                                                                                                                                                                                              |
| ---------------------------- | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `infra/envs/production.json` | 3    | All three keys deleted — `enableWaf`, `cloudFrontWafRateLimit`, `apiWafRateLimit` — together with their `types.ts` declarations and the two `< 100` validations that would otherwise have forced the rate limits to stay. (Superseded plan: flip `enableWaf` to `false` and leave the limits in place.) |

### Decision records with stale pointers

| File                                                                         | What to do                                                                                                                                                                                                                                                                              |
| ---------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lore/2-adrs/0048_cloudflare-edge-over-aws-waf.md`                           | Record that Decision 5 is executed. Fix three stale references: the acceptance note defers to "backlog 0283" (a different task after the ID sweep — this is 0302); `links:` and the body point at `lore/1-tasks/active/0277_…` but 0277 is archived; `related_tasks` omits 0302.        |
| `lore/1-tasks/archive/0277_FEATURE_migrate-edge-protection-to-cloudflare.md` | One line: the future-work list still points at "**0283** — Drop AWS WAF after soak". Pointer only — the rest is a historical record and stays.                                                                                                                                          |
| `lore/2-adrs/0001_OIDC-cicd-and-public-repo-secret-separation.md`            | Broken link to `../1-tasks/backlog/0072_FEATURE_cdk-cloudfront-waf-route53.md`; that task is now `archive/0035_FEATURE_cdk-cloudfront-waf-route53.md`.                                                                                                                                  |
| `lore/1-tasks/backlog/0090_FEATURE_security-audit.md`                        | Two audit criteria — "WAF active on API Gateway" and "[ ] WAF/throttling active on public ingress" — become unsatisfiable as written. Restate against the Cloudflare edge + API Gateway throttling. (The same task also still checks for RDS, which is retired; unrelated, left alone.) |

## Out of scope — no change

| File(s)                                                                                                                              | Why                                                                                                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/scf/**` (14 hits: evidence, security checklist, form answers, video scenario, load-test CSVs, `architecture-m2-read-path.mmd`) | Milestone-3 submission material, reconciled separately. The `.mmd` matters: it renders to Figure 1 in the evidence document and its caption asserts the launch controls.            |
| `docs/runbooks/backfill_soroban_2of5_fresh_machine.md`                                                                               | "Cloudflare WAF on `mainnet.sorobanrpc.com`" — a third party's WAF blocking us, nothing of ours.                                                                                    |
| `infra/cloudflare/README.md`, `infra/cloudflare/dns.tf`                                                                              | Describe Cloudflare's own zone-level WAF / rate-limit / challenge rulesets. Unaffected and still true.                                                                              |
| `lore/1-tasks/archive/**` (0035, 0097, 0338, 0405, 0273, 0004, 0006, 0033, 0038, 0066, 0239)                                         | Historical records of work already done. Rewriting them would falsify the record. Only 0277's forward pointer is corrected, because it points at a task that still has to be found. |
| `lore/2-adrs/0053_fast-change-offchain-compute-at-read.md`                                                                           | Cites ADR 0048 by filename in a history note. Still accurate.                                                                                                                       |

## The WAF construct code itself — deleted

Decided 2026-07-27, during the task rather than after it: the constructs go with
the WebACLs.

Removed: `infra/src/lib/constructs/waf-web-acl.ts`,
`infra/src/lib/stacks/cloudfront-waf-stack.ts`, the `config.enableWaf` gates in
`app.ts` / `api-gateway-stack.ts` / `delivery-stack.ts`, the two `< 100`
validations, and `enableWaf` / `cloudFrontWafRateLimit` / `apiWafRateLimit` from
both `types.ts` and `infra/envs/production.json`.

Reasoning: keeping the code only made sense as a re-arm path, and there is nothing
to re-arm for. The frontend is deliberately left without edge control — an accepted
decision, not a temporary state — and fronting CloudFront with Cloudflare would
stack two CDNs. A dormant flag with no intended use is dead flexibility, and this
particular flag carries a footgun: flipping it back recreates a cross-region stack
whose deletion is the awkward step documented in the parent task.

Two consequences worth knowing:

- Removing `cloudFrontWafArn` takes `crossRegionReferences: true` with it. That was
  the **only** cross-region reference in the app; after this the whole CDK app is
  single-region except for the ACM certificate ARN, which is just a string.
- `api-gateway-stack.ts` keeps a `CfnOutput` for the WebACL ARN further down the
  file, well away from the construct. Deleting only the `const waf = …` declaration
  leaves it dangling and `tsc` fails with `TS2304: Cannot find name 'waf'`. It
  builds clean once both go.
