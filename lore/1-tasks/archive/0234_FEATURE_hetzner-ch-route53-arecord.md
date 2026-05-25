---
id: '0234'
title: 'FEATURE: Route 53 ARecord for Hetzner ch-prod in production CDK'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0227', '0216']
tags:
  [priority-medium, effort-small, layer-infrastructure, aws-cdk, dns, hetzner]
links: []
history:
  - date: '2026-05-18'
    status: backlog
    who: fmazur
    note: 'Spawned from 0227 future work — DNS for Hetzner CH endpoint deferred so initial deploy can validate everything else without blocking on CDK changes outside the 0227 scope.'
  - date: '2026-05-20'
    status: active
    who: fmazur
    note: 'Promoted to active. Pre-requirement for 0239 (AWS cutover) Phase 4-5. Unblocks dev-laptop mTLS access to Hetzner CH once LE cert is issued from a real DNS name.'
  - date: '2026-05-20'
    status: completed
    who: claude
    note: >
      Code-side implementation done: HetznerDnsStack (Route 53 ARecord
      with IP from SSM Parameter Store), `chDomainName` field in
      EnvironmentConfig (per-env, neutral naming — no "prod" in name),
      Makefile targets per-env, CH_PROD_DOMAIN → CH_DOMAIN rename across
      Caddyfile / Compose / Ansible / README. 14 files, +111/-19 lines.
      docs/architecture/infrastructure/infrastructure-overview.md §5.6
      updated per ADR 0032. Deferred operator-side: SSM put-parameter,
      hostedZoneId fill-in, password-manager entry rename, caddy-data
      volume wipe, post-deploy smoke tests.
---

# FEATURE: Route 53 ARecord for Hetzner ch-prod in production CDK

## Summary

Add an A record `ch-prod.sorobanscan.rumblefish.dev` pointing to the
Hetzner ch-prod-01 dedicated server's public IPv4. Without this
record the Hetzner Caddy cannot obtain a Let's Encrypt certificate
(HTTP-01 challenge requires public DNS resolution to the box), and
external clients (AWS Lambda, dev laptops, Galexie) cannot establish
mTLS connections to the production ClickHouse endpoint.

## Context

Task [0227](../active/0227_FEATURE_infra-hetzner-ansible-playbook.md)
delivered the Hetzner deployment artefacts (Ansible playbook, Caddy
config, mTLS CA, Docker compose overlay). Initial deploy used a
placeholder `ch-prod_domain` so Caddy entered an LE retry loop
without affecting the rest of the stack — this is the canonical
"deploy without DNS" mode documented in the runbook.

The runbook explicitly defers the DNS configuration to a follow-up
because:

- Production Route 53 hosted zone `sorobanscan.rumblefish.dev`
  exists in AWS Console but `infra/envs/production.json` still has
  `hostedZoneId: "CHANGE_ME"` — the production CDK has not been
  wired up to the zone yet.
- Adding a Route 53 ARecord lives in `infra/src/` (AWS CDK), which
  task 0227 explicitly marks as out-of-scope ("AWS-side cutover
  — separate work in `infra/src/`").
- The team's workflow is "infra deployed from folder, not clicked";
  a manual Route 53 ARecord would break that consistency.

## Scope

### Changes in `infra/`

- `infra/envs/production.json`:

  - Replace `hostedZoneId: "CHANGE_ME"` with the real ID of the
    `sorobanscan.rumblefish.dev` hosted zone (visible in AWS Console
    → Route 53 → Hosted zones).
  - Add a new field for the Hetzner CH endpoint, e.g.:
    ```json
    "chProdDomainName": "ch-prod.sorobanscan.rumblefish.dev",
    "chProdIp":         "<dedicated-server-ipv4>"
    ```
  - Update `EnvConfig` interface in `infra/src/lib/types.ts` to
    expose these fields.

- New stack `infra/src/lib/stacks/hetzner-dns-stack.ts`:

  - Standalone CDK stack with a single `route53.ARecord`.
  - `target: RecordTarget.fromValues(chProdIp)` (literal IP, not
    AWS alias — the box is non-AWS).
  - `ttl: Duration.minutes(5)` so future IP changes propagate fast.
  - Comment field flagging "non-AWS target — Hetzner Falkenstein
    dedicated server".

- Wire up in `infra/src/bin/production.ts`:
  - Instantiate `HetznerDnsStack` as a separate stack so it can be
    deployed independently of frontend / api / compute stacks.

### Changes in `infra-hetzner/`

- Operator's `~/.config/soroban-prod.env` (and the
  `soroban-prod / ansible-env` password-manager entry):

  - `CH_PROD_DOMAIN`: change from `ch-prod.placeholder` to
    `ch-prod.sorobanscan.rumblefish.dev`.
  - `ACME_EMAIL`: switch from the validation-phase placeholder
    (whatever was used during 0227 initial deploy — possibly a
    fake / test address that worked because LE never reached
    account-creation while the domain was a placeholder) to a
    real, monitored inbox. Options:
    - personal email of the on-call operator
    - team alias forwarded to Slack / Discord (e.g.
      `infra-alerts@rumblefish.dev` if such alias exists)
    - shared `ops@rumblefish.dev` if the team operates one
  - The chosen address receives LE expiry warnings only when
    auto-renewal fails (rare); make sure it is a recipient that
    someone will actually read.

- After updating env values and re-sourcing, **wipe the
  `caddy-data` Docker volume on the box once** so Caddy creates
  a fresh LE account with the new email. The Compose project
  name on the box is `app` (derived from `project_src=/srv/app`
  basename — see `app_repo_dest` in `group_vars/all.yml`), so
  the fully-qualified volume name is `app_caddy-data`. **Do
  not** copy the repo name here — the box does not see the
  GitHub project name, only `/srv/app`.

  ```bash
  ssh deploy@ch-prod-01 \
      sudo docker compose -f /srv/app/docker-compose.yml \
                          -f /srv/app/docker-compose.prod.yml \
                          stop caddy
  ssh deploy@ch-prod-01 \
      sudo docker volume rm app_caddy-data
  ```

  (Without the wipe, Caddy keeps using the original LE account
  with the placeholder email. LE has no Caddy-side command to
  update the email of an existing account.)

- Re-deploy Ansible: `ansible-playbook -i inventory.ini site.yml`.
  Caddy picks up the new domain, performs first successful LE
  challenge, creates a fresh LE account with the real email,
  obtains the production cert.

## Acceptance Criteria

- [ ] `production.json` `hostedZoneId` no longer `CHANGE_ME`
      — deferred to operator (user explicitly chose to leave
      `CHANGE_ME` placeholder; `validateConfig` blocks `cdk synth`
      until it is filled in — intentional safety net)
- [ ] `HetznerDnsStack` deployed; `aws route53 list-resource-record-sets`
      shows the new A record — deferred to operator (post-merge,
      after `aws ssm put-parameter` + `hostedZoneId` fill-in)
- [ ] `dig +short ch.sorobanscan.rumblefish.dev` returns the
      box's IPv4 within 5 minutes of CDK deploy (TTL-bound)
      — deferred to operator (post-deploy verification)
- [ ] Caddy obtains LE cert successfully — `docker logs caddy`
      shows `certificate obtained successfully` for the new domain
      — deferred to operator (requires `CH_DOMAIN` env rename in
      password manager + `caddy-data` volume wipe + re-deploy)
- [ ] LE account email is a real monitored address (not the
      validation-phase placeholder); verified by inspecting the
      `caddy-data` volume's account JSON or by issuing a fresh
      cert and confirming the LE account on the request was
      re-created with the new email
      — deferred to operator (password manager update)
- [ ] External mTLS smoke test from operator laptop succeeds **without**
      `--insecure`:

      ```bash
      curl --cert ~/.certs/<dev>-laptop.crt \
           --key  ~/.certs/<dev>-laptop.key \
           --cacert infra-hetzner/ca/ca.crt \
           "https://ch.sorobanscan.rumblefish.dev/?query=SELECT+version()&user=default&password=$CLICKHOUSE_PASSWORD"
      ```

      — deferred to operator (post-deploy)

- [ ] **Negative mTLS smoke test** — connection without a client
      certificate is rejected at the TLS-handshake stage (carry-over
      from 0227's Phase 6 acceptance, deferred here because LE cert
      issuance unblocks both tests in the same run):

      ```bash
      curl -sv --cacert infra-hetzner/ca/ca.crt \
           "https://ch.sorobanscan.rumblefish.dev/" 2>&1 \
        | grep -iE "alert|certificate required|tls.*error"
      ```

      Expect a TLS alert (`handshake failure` / `certificate
      required`) and a non-zero exit.
      — deferred to operator (post-deploy)

- [x] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`
- [x] **Docs updated** — - `infra-hetzner/README.md`: line 86 (`CH_PROD_DOMAIN` placeholder
      caveat → `CH_DOMAIN` with real example); Caddy/LE operating
      note updated to point at `HetznerDnsStack`. The original task
      wording "no domain yet, Caddy in LE retry loop" caveat was not
      present in README at time of work (grep confirmed); the
      "placeholder OK until task 0234" wording was updated instead. - `infra/README.md`: new section "HetznerDnsStack — Route 53
      record for the production ClickHouse box" with SSM bootstrap
      and deploy commands. - `docs/architecture/infrastructure/infrastructure-overview.md`
      §5.6 updated with a Route 53 + literal-IP paragraph (per ADR
      0032 — PR changes shape of the system, docs follow in same PR).

## Implementation Notes

**Files (14 total: 13 modified, 1 new; +111/-19 lines):**

- New: `infra/src/lib/stacks/hetzner-dns-stack.ts` — `HetznerDnsStack`
  with `route53.HostedZone.fromHostedZoneAttributes`, `ssm.StringParameter.valueForStringParameter`
  for the IP, `route53.ARecord` (5-min TTL, literal-IP target).
- `infra/src/lib/types.ts`: added `readonly chDomainName: string` to
  `EnvironmentConfig` + placeholder/CHANGE blocker in `validateConfig`.
  `chIp` intentionally NOT in the interface — read from SSM at stack
  build time.
- `infra/src/lib/app.ts`: 1-line wire-up `new HetznerDnsStack(app, ...)`.
- `infra/envs/production.json`: `chDomainName: "ch.sorobanscan.rumblefish.dev"`.
- `infra/envs/staging.json`: `chDomainName: "PLACEHOLDER"` (forces
  synth failure on staging — that environment is slated for AWS
  decommissioning, see project memory `prod-store-ch-on-hetzner`).
- `infra/Makefile`: `synth/diff/deploy-{staging,production}-hetzner-dns`
  targets (symmetric with existing per-env, per-stack targets).
- `infra/README.md`: new "HetznerDnsStack" section with SSM
  put-parameter bootstrap + deploy.
- Hetzner-side rename `CH_PROD_DOMAIN → CH_DOMAIN` / `ch_prod_domain →
ch_domain` in: `infra-hetzner/Caddyfile`, `infra-hetzner/README.md`
  (5 spots), `infra-hetzner/ansible/group_vars/all.yml`,
  `infra-hetzner/ansible/roles/app/templates/env.j2`,
  `infra-hetzner/ansible/site.yml` (preflight assert + fail_msg),
  `docker-compose.prod.yml` (passthrough env var).
- `docs/architecture/infrastructure/infrastructure-overview.md`:
  paragraph in §5.6 about Route 53 / `HetznerDnsStack`.

**Build:** `nx build @rumblefish/soroban-block-explorer-aws-cdk` ✅

## Design Decisions

### From Plan

1. **Standalone CDK stack** so it deploys independently of other
   stacks — per task spec ("frontend / api / compute"). Stack name
   `Explorer-${envName}-HetznerDns`, deployable with
   `make deploy-${env}-hetzner-dns`.

2. **Literal IPv4 target** (`RecordTarget.fromValues`, not
   `fromAlias`) — per spec, because the box is non-AWS.

3. **TTL 5 minutes** so post-box-replacement IP changes propagate
   fast — per spec.

4. **Comment field** flagging "non-AWS target — Hetzner dedicated
   server" — per spec, for future operator reading the record in
   AWS Console.

### Emerged

5. **Neutral field naming**: original spec wording was
   `chProdDomainName` / `chProdIp`. Renamed to `chDomainName` /
   (eventually no `chIp` — see #7). Reason: user requested env-driven
   naming ("nie nazywaj tego prod, tylko zamiast prod ma być wartość
   envu"). The `envName` field already distinguishes prod from
   staging, so duplicating "prod" in the field name was redundant.

6. **Domain value** also de-prefixed: `ch-prod.sorobanscan.rumblefish.dev`
   → `ch.sorobanscan.rumblefish.dev`. Required Hetzner-side rename
   `CH_PROD_DOMAIN` → `CH_DOMAIN` (Caddyfile, Ansible group_vars,
   env.j2 template, site.yml preflight assert, docker-compose
   passthrough, README) for consistency.

7. **`chIp` removed from env config; read from SSM Parameter Store
   instead**: discovered during safety review that
   `infra-hetzner/ansible/inventory.ini` is gitignored — existing
   convention keeps box-specific IPs out of git. Committing
   `chIp: "1.2.3.4"` to `production.json` would have broken that
   convention. Final design: `ssm.StringParameter.valueForStringParameter(this, '/soroban/${envName}/ch-ip')`
   renders a CFN dynamic reference (`{{resolve:ssm:...}}`); CFN
   resolves at deploy time so `cdk synth` needs no AWS auth and the
   IP never lands in `cdk.out/*.template.json`. Operator bootstraps
   the parameter once via `aws ssm put-parameter` or AWS Console
   (Systems Manager → Parameter Store).

8. **`chDomainName` kept mandatory** (not optional): user requested
   both env configs have the field (staging gets `PLACEHOLDER`).
   `validateConfig` blocks `cdk synth` on the placeholder — that's
   intentional, since staging is slated for decommissioning and
   should NOT deploy a Hetzner DNS record.

9. **No `chIp` validation in `validateConfig`**: removed once the
   value moved to SSM. The IPv4-regex check could not validate a
   CFN token; the SSM `put-parameter` is the gate now. If the
   parameter is missing at deploy time, CFN fails with a clear
   error referencing the parameter name.

10. **Stack always created (no guard in `app.ts`)**: with
    `chDomainName` mandatory, there's no need for the original
    optional-field guard. Staging is blocked at `validateConfig`
    before stack instantiation.

11. **Updated `docs/architecture/infrastructure/infrastructure-overview.md`
    §5.6** — not explicitly listed in the task spec, but ADR 0032
    (project CLAUDE.md) requires evergreen architecture docs to
    track any PR that changes the shape of the system. This PR
    adds a new publicly-exposed DNS endpoint and a new CDK stack,
    so the §5.6 paragraph was added in the same commit set.

## Issues Encountered

- **`CH_PROD_DOMAIN → CH_DOMAIN` rename is a breaking change** for
  every operator deploy environment. Mitigation: each layer
  (Compose `${VAR:?…}`, Ansible preflight assert, Caddy substitution)
  fail-fasts loudly with the new variable name, so a stale env file
  on someone's laptop manifests as "missing CH_DOMAIN" rather than
  silent drift. Operator action documented in this task's Scope
  section (password manager + ansible-env entry update).

- **Project memory "don't touch infra/"** (2026-05-13) appeared to
  conflict with this task. Resolved: that memory predates this
  task's explicit activation (2026-05-20) and named Filip as the
  party who would later handle the AWS cutover — task 0234 IS that
  cutover work, so the memory's intent is satisfied by activating
  this task, not by abstaining from `infra/`.

- **Original task "no domain yet, Caddy in LE retry loop" caveat
  removal** — that exact wording was not present in
  `infra-hetzner/README.md` at time of work (grep `-iE "no domain
yet|retry loop"` returned nothing). The closest match was
  "placeholder OK until task 0234" on line 86, which was updated
  to point at the new `CH_DOMAIN` example.

- **AWS account ID `750702271865` in `staging.json:45`** is
  pre-existing (from commit `25c93c9 feat(lore-0191): SQS-driven
type-1 icon_url enrichment`). Not introduced by this PR, but
  flagged in the safety review — if the org treats account IDs as
  quietly-sensitive, that's a separate hygiene task.

## Future Work — operator-side, not spawned to backlog

These are the deferred AC items from above, listed here for
operator handoff. NOT spawned as separate backlog tasks (per
project convention: keep operator handoff as prose, owner-driven
backlog spawn).

1. AWS Console / CLI: `aws ssm put-parameter --name /soroban/production/ch-ip --value <ipv4> --type String --region us-east-1`.
2. Fill `hostedZoneId` in `infra/envs/production.json` with the
   real Z-id from AWS Console → Route 53 → Hosted zones.
3. Password manager: update `soroban-prod / ansible-env` entry to
   rename `CH_PROD_DOMAIN` → `CH_DOMAIN`, set value
   `ch.sorobanscan.rumblefish.dev`, set `ACME_EMAIL` to a real
   monitored address.
4. `make deploy-production-hetzner-dns` (after #1 + #2).
5. On the box: wipe `app_caddy-data` volume (commands in Scope
   section), re-source env, `ansible-playbook -i inventory.ini
site.yml`.
6. Smoke tests: positive + negative mTLS curl from operator
   laptop (commands in AC section).

## Out of Scope

- Other production CDK stacks (delivery, api-gateway, compute, etc.)
  — those are separate work tied to the full AWS-side cutover.
- DNS-01 wildcard certificates for `*.sorobanscan.rumblefish.dev`
  — current task uses HTTP-01 for a single hostname.
- Migrating Hetzner DNS away from Route 53 to Hetzner DNS — not
  planned; Route 53 stays as the single source of truth for the
  rumblefish.dev subzones.

## Dependencies

- Task 0227 must be deployed at least once (so the box IP is
  known and the playbook validated end-to-end without DNS).
- AWS production hosted zone for `sorobanscan.rumblefish.dev`
  must exist and be reachable from CDK (it does — verified in
  AWS Console as of 2026-05-18).
