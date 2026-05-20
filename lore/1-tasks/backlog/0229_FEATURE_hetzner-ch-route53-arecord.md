---
id: '0229'
title: 'FEATURE: Route 53 ARecord for Hetzner ch-prod in production CDK'
type: FEATURE
status: backlog
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
- [ ] `HetznerDnsStack` deployed; `aws route53 list-resource-record-sets`
      shows the new A record
- [ ] `dig +short ch-prod.sorobanscan.rumblefish.dev` returns the
      box's IPv4 within 5 minutes of CDK deploy (TTL-bound)
- [ ] Caddy obtains LE cert successfully — `docker logs caddy`
      shows `certificate obtained successfully` for the new domain
- [ ] LE account email is a real monitored address (not the
      validation-phase placeholder); verified by inspecting the
      `caddy-data` volume's account JSON or by issuing a fresh
      cert and confirming the LE account on the request was
      re-created with the new email
- [ ] External mTLS smoke test from operator laptop succeeds **without**
      `--insecure`:

      ```bash
      curl --cert ~/.certs/<dev>-laptop.crt \
           --key  ~/.certs/<dev>-laptop.key \
           --cacert infra-hetzner/ca/ca.crt \
           "https://ch-prod.sorobanscan.rumblefish.dev/?query=SELECT+version()&user=default&password=$CLICKHOUSE_PASSWORD"
      ```

- [ ] **Negative mTLS smoke test** — connection without a client
      certificate is rejected at the TLS-handshake stage (carry-over
      from 0227's Phase 6 acceptance, deferred here because LE cert
      issuance unblocks both tests in the same run):

      ```bash
      curl -sv --cacert infra-hetzner/ca/ca.crt \
           "https://ch-prod.sorobanscan.rumblefish.dev/" 2>&1 \
        | grep -iE "alert|certificate required|tls.*error"
      ```

      Expect a TLS alert (`handshake failure` / `certificate
      required`) and a non-zero exit.

- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`
- [ ] **Docs updated** — `infra-hetzner/README.md` operating-model
      section: remove the "no domain yet, Caddy in LE retry loop"
      caveat once the real domain is wired up

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
