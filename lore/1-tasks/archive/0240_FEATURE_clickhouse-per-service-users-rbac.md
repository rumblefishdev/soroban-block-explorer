---
id: '0240'
title: 'FEATURE: ClickHouse per-service users + RBAC profiles + quotas (Layer 3 defense-in-depth)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0216', '0227', '0239', '0250']
tags:
  [
    priority-high,
    effort-medium,
    layer-infrastructure,
    clickhouse,
    rbac,
    security,
    defense-in-depth,
  ]
milestone: 1
links: []
history:
  - date: '2026-05-20'
    status: backlog
    who: fmazur
    note: 'Spawned during 0238 scope review — 0216 decision document specifies a three-layer defense-in-depth (TLS + mTLS + CH-side RBAC) but 0227 delivered only Layers 1–2. Currently every mTLS-authenticated client reaches CH as the single `default` user with full permissions. This task adds Layer 3: per-service CH users, profiles and quotas matching each AWS service / dev consumer.'
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: 'Activated. Pulled ahead of [[task-0239]] so the Secrets Manager bundle contract and per-service CH users land before 0239 Phase 1 starts issuing certs and Phase 2 starts wiring Lambda runtime — avoids a two-phase bundle rollout + Lambda re-deploy.'
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Redesigned: dropped HTTP Basic Auth + per-service passwords in
      favour of Caddy CN → CH user `map` + `<no_password/>` users
      restricted to loopback. Identity proven by mTLS cert; passwords
      removed from the design entirely. Trade-off: one fewer defense
      layer (compromised host → full CH access without needing SM
      passwords), accepted for dedicated single-tenant Hetzner box.
      Gated on three empirical verifications (Caddy header strip, CH
      no-password loopback auth, CH listen address). Bundle for
      [[task-0239]] stays `{cert, key, ca}` (no `ch_user`/`ch_password`
      fields needed).
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Dropped per-developer CH users (`<firstname>_dev`). Dev certs
      map to `default` in Caddy; `default` becomes `<no_password/>` +
      loopback-restricted so cert alone suffices. Removes `devs.xml`,
      `dev_limited` profile, `dev_throttle` quota and the Ansible env
      var for default's password. Accepted trade-off: any dev cert =
      full DDL on CH. Justification: devs already need admin for
      debugging / ad-hoc migrations, and cert revocation (remove CN
      from Caddy map) is the same single-edit operation either way.
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Phase 0 verifications PASSED on isolated CH 26.3 + Caddy 2.10
      sandbox. V1: Caddy `header_up X-ClickHouse-User {ch_user}` in
      `set` mode already replaces client-supplied header — explicit
      `-` strip is unnecessary, spec simplified. V2: `<no_password/>`
      user accepts `X-ClickHouse-User` header without Basic Auth;
      `default` override REQUIRES `replace="replace"` attribute on
      the `<default>` element to override the image entrypoint
      (without it the override silently merges and the password
      requirement persists). V3: prod compose binds CH host port to
      `127.0.0.1` already, but Caddy → CH source IP is the compose
      bridge gateway, so `<networks>` must pin the bridge subnet.
      Decision: pin compose subnet explicitly in
      `docker-compose.prod.yml` rather than relying on Docker's
      default range, and add an ufw rule blocking :8123 + :9000 from
      external interfaces as defense-in-depth.
  - date: '2026-05-21'
    status: completed
    who: fmazur
    note: >
      Delivered across 6 phases (0–5) plus a 2.5 Model B refactor and
      a docs trim pass. NEW files: 3 CH XML configs
      (profiles.xml/quotas.xml/services.xml, 5 profiles + 4 quotas +
      6 proxy-trust users including dev_shared), 1 Ansible Jinja
      template (cn_user_map.snippet.j2), 1 architecture doc
      (docs/architecture/security/clickhouse-rbac.md, 152 lines after
      threat-model trim), 1 follow-up backlog spec
      (lore/1-tasks/backlog/0250 — quota enforcement investigation).
      DELETED: cn_allowlist.snippet.j2 (replaced by map mechanism).
      MODIFIED: 10 files (docker-compose dev+prod, Caddyfile,
      infra-hetzner/README.md, group_vars/all.yml, app role tasks,
      env.j2, clickhouse-client.xml.j2, site.yml,
      infrastructure-overview.md). Auth model: hybrid — `default`
      keeps password (host-side admin: sidecar / backup / SSH→docker
      exec); 6 proxy-trust users are `<no_password/>` restricted to
      compose bridge 172.30.0.0/16 + loopback. Caddy maps verified
      mTLS CN → CH user via `header_up X-ClickHouse-User {ch_user}`.
      Empirical findings: (a) CH 26.3 refuses SQL GRANT on XML users
      (ACCESS_STORAGE_READONLY) — init.sql GRANT narrowing
      unreachable; (b) CH 26.3 quota counters do NOT increment for
      `X-ClickHouse-User` header path — DoS protection delegated to
      AWS API GW / profile execution caps / firewall; documented and
      follow-up [[task-0250]] spawned. Dev compose smoke verified
      twice (Phase 2 + Model B refactor). Operator env var change:
      `ALLOWED_CLIENT_CNS` → `CLICKHOUSE_CN_USER_MAP` (breaking, must
      update operator shell env before next deploy). Phase 6
      (operator deploy on live box, including host firewall) is
      tracked separately; this PR ships the code + docs only.
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Phase 5 (docs) implemented: NEW
      `docs/architecture/security/clickhouse-rbac.md` documents the
      full RBAC system (auth model split, per-service user matrix,
      Caddy CN→user mapping, profile/quota definitions,
      partition_admin caveat, known limitations including the
      proxy-trust quota gap, rotation + revocation procedures,
      audit trail). `docs/architecture/infrastructure/infrastructure-overview.md`
      §5.6 gets a 3-paragraph pointer to the new security doc.
      `infra-hetzner/README.md` Operating Notes adds an RBAC
      pointer section; password rotation section narrows scope to
      `default` host-side; Adding/Removing developer rewritten to
      reflect env-var-driven workflow
      (`OPERATOR_SSH_PUBKEYS` + `CLICKHOUSE_CN_USER_MAP`, no
      group_vars hand-edits); env-var listing updates
      `ALLOWED_CLIENT_CNS` → `CLICKHOUSE_CN_USER_MAP`; smoke-test
      script comment updated to reference `map` allowlist instead
      of removed `cn_allowlist.snippet`.
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Phase 4 (quota smoke + cert revocation E2E) — partial pass.
      Cert revocation gating already validated in Phase 0 V1 (Caddy
      `map` with unmapped CN → 403 before backend hop), not
      re-tested. Quota smoke surfaced a major CH 26.3.10 gotcha:
      `X-ClickHouse-User` header path does NOT increment quota
      counters (verified empirically — counter stays at 0 after
      10001 queries). URL param `?user=` and TCP native auth DO
      count. CH refuses to mix the two (`Invalid authentication`),
      so the obvious "set both" workaround is blocked. Quota
      enforcement is effectively no-op for our proxy-trust traffic
      path; DoS protection moves to other layers (AWS API GW
      throttle, Caddy `request_body max_size`, profile
      max_execution_time). Documented as known limitation in spec
      Risks + adjusted AC; quotas remain defined in `quotas.xml`
      because (a) host-side path uses them, (b) docs intent, (c)
      future CH upgrade or Caddy URL-rewrite can restore enforcement
      without config rewrite.
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      After Phase 2 implementation, reconsidered the "remove password
      everywhere" simplification and reverted to a hybrid auth model:
      `default` keeps its image-entrypoint-managed password (used by
      sidecar, backup script, SSH→docker exec) and a NEW `dev_shared`
      admin user (`<no_password/>` + loopback/bridge networks) takes
      over from `default` as the target for dev cert mappings. Caddy
      proxy-trust still drives external clients (service certs to
      scoped users, dev certs to `dev_shared`); host-side operations
      still know the password. The realistic security delta is small
      (deploy user with SSH can read `/srv/app/.env` either way), but
      keeping the password preserves the option for tighter future
      postures (compliance, SSH-without-docker-group, public CH
      endpoint) without a re-rollout. Reverted CLICKHOUSE_PASSWORD
      in env.j2 / group_vars / site.yml / docker-compose.{yml,prod.yml}
      / clickhouse-client.xml.j2. services.xml drops the
      `<default replace="replace">` override (image entrypoint
      reclaims management) and adds `<dev_shared>` instead.
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Phase 1 (CH-side static config) implemented + verified.
      `profiles.xml`, `quotas.xml`, `services.xml` created in
      `crates/db-clickhouse/users.d/`. All 6 users, 5 profiles, 4
      quotas register correctly in `system.{users,settings_profiles,
      quotas}`. Functional negative tests pass: api_reader DROP
      TABLE → READONLY; galexie ALTER → QUERY_IS_PROHIBITED.
      Emergent finding during Phase 3 prep: CH 26.3 refuses SQL
      `GRANT`/`REVOKE` against XML-defined users with
      `ACCESS_STORAGE_READONLY`. The planned init.sql GRANT-based
      narrowing of `partition_admin` is therefore unreachable —
      partition_admin remains profile+quota gated (300s exec, 100
      Q/h). Spec updated to reflect this; init.sql section is now
      a no-op. Effective Phase ordering: Phase 0 (done) → Phase 1
      (done) → Phase 2 (Ansible + compose mount, pending) → Phase 4
      (quota smoke + cert revocation E2E test) → Phase 5 (docs).
---

# FEATURE: ClickHouse per-service users + RBAC profiles + quotas

## Summary

Replace the single shared `default` ClickHouse user with **per-service
users**, each bound to a profile (read-only / write-no-DDL / admin) and
a quota (queries-per-hour, memory, execution time). Identity is proven
by the mTLS cert verified at Caddy: the verified CN is mapped to a CH
user via Caddy's `map` directive and forwarded as
`X-ClickHouse-User`. CH users are `<no_password/>` and restricted to
loopback, so the cert is the only credential anywhere in the path.
Closes the defense-in-depth Layer 3 gap left by 0227.

## Context

The three-layer security architecture decided in [[task-0216]] is:

1. **Layer 1** — TLS encryption (Caddy + Let's Encrypt) ✅ done in
   [[task-0227]]
2. **Layer 2** — mTLS client cert verify + CN allowlist (Caddy
   `client_auth.mode require_and_verify` + Ansible-rendered snippet)
   ✅ done in [[task-0227]]
3. **Layer 3** — ClickHouse user + RBAC + quotas — **not yet
   implemented**

Without Layer 3, the three "independent layers" collapse to two
effective layers, and a compromise of any single mTLS client cert
(Lambda IAM principal stolen, dev laptop unrevoked, Galexie task
hijacked) yields the same blast radius as compromising the
shared CH password: full read/write/DDL on every table.

### Auth model: proxy-trust, not HTTP Basic Auth

The original draft of this task gave every service its own CH password
delivered through a Secrets Manager bundle (`{cert, key, ca, ch_user,
ch_password}`). This redesign drops passwords entirely:

- Caddy verifies the mTLS cert chain (as today) and uses a `map`
  directive to translate the verified CN into a CH user name.
- Caddy strips any client-supplied `X-ClickHouse-User` header and sets
  it from the map, so identity cannot be spoofed by the client.
- CH listens on loopback (or the docker bridge gateway — see Phase 0
  V3) and accepts `<no_password/>` users from that source IP.
- A host firewall rule blocks :8123 from any other source as a
  backstop in case the listen-address config drifts.

Why proxy-trust instead of HTTP Basic Auth:

- **Single source of identity** — the cert. No drift risk between
  a Lambda's bundle and CH's user table; no rotation procedure where
  forgetting one of N updates breaks a single service.
- **Cert rotation IS identity rotation** — no separate per-service
  password rotation procedure.
- **Bundle for [[task-0239]] stays `{cert, key, ca}`** — Lambda /
  Galexie code in 0239 Phase 2 only configures the HTTP client with
  the cert; no Basic Auth header, no extra secret-bundle fields.
- **Fewer secrets in flight** — no plaintext passwords in Secrets
  Manager bundles or on the Hetzner box's env file.

Security trade-off: proxy-trust elides one defense layer. A
compromised host with loopback access to CH can present itself as any
CH user without further credentials. Accepted for our setup —
dedicated Hetzner box, no co-tenants, host compromise already implies
access to `~/.config/soroban-prod.env` (which holds the same
passwords in the original design). Three empirical verifications
guard the proxy-trust path before it ships — see Phase 0.

This task is a **strict pre-requirement of [[task-0239]] Phase 6
step 2** (drop RDS). Before AWS-side compute starts depending on
ClickHouse as the sole datastore, RBAC must constrain blast radius
on a per-service basis.

## Scope

### Phase 0 — Empirical verifications (gating)

These three must pass on dev compose **before** any user-facing
change ships. If any fails, redesign back to HTTP Basic Auth (~1 day
sunk cost; see git history of this file for the prior design).

**V1. Caddy `header_up` strip-then-set actually strips client-supplied header.**
Load-bearing security check. If this fails, any client with a valid
cert can present as `migration_admin` and bypass RBAC entirely.

```bash
curl -k --cert client.crt --key client.key \
  -H "X-ClickHouse-User: migration_admin" \
  -H "X-ClickHouse-User: another" \
  https://localhost/?query=SELECT+currentUser()
```

Expected: response is the cert's mapped user (e.g. `api_reader`),
NOT `migration_admin` or `another`. Duplicate headers test that
Caddy strips every instance before setting its own.

**V2. CH 26.3 accepts `<no_password/>` user with `X-ClickHouse-User` header from loopback.**

```bash
# Directly against CH, bypassing Caddy:
curl http://127.0.0.1:8123/?query=SELECT+currentUser() \
  -H "X-ClickHouse-User: api_reader"
```

Expected: `api_reader`. No Basic Auth header, no `password` URL param.

Note: an extension of this test (`X-ClickHouse-User: default` after
applying an in-`users.d` override with `<default replace="replace">`

- `<no_password/>`) also succeeded, proving the override mechanism
  works. The final model does NOT use this mechanism — `default` is
  left to the image entrypoint (password from `CLICKHOUSE_PASSWORD`
  env), and dev certs map to a new `dev_shared` user instead. The
  override-replace finding is documented for future use should the
  posture ever flip.

**V3. Caddy → CH source IP matches CH `<networks>` config.**

Caddy and CH may run in `network_mode: host` (source IP = 127.0.0.1)
or a docker bridge (source IP = bridge gateway). Determine which,
configure each user's `<networks>` accordingly, and assert in Ansible
that the rendered `config.d/listen.xml` binds only to the expected
interface. Add a host firewall rule (ufw / nftables) dropping :8123
from non-loopback / non-Caddy sources as defense-in-depth.

If all three pass — proceed with Phase 1. Otherwise — fall back to
the original HTTP Basic Auth design and re-plan.

### Per-service user matrix

Final layout — one CH user per AWS service + per developer:

| CH user                   | Profile          | Quota          | Permitted operations                                                   | Consumer                                               |
| ------------------------- | ---------------- | -------------- | ---------------------------------------------------------------------- | ------------------------------------------------------ |
| `default` (image-managed) | `default`        | `default`      | Everything (admin, password from `CLICKHOUSE_PASSWORD` env)            | `db-clickhouse-init` sidecar + backup script + SSH ops |
| `dev_shared`              | `admin`          | `unlimited`    | Everything (admin, `<no_password/>` + loopback/bridge networks)        | Dev laptops (one shared cert-gated user)               |
| `galexie`                 | `write_no_ddl`   | `high_write`   | INSERT only on ingestion tables                                        | Galexie ECS task                                       |
| `api_reader`              | `read_only`      | `api_throttle` | SELECT on `default.*`                                                  | Lambda API (read-heavy)                                |
| `ingestion_writer`        | `write_no_ddl`   | `high_write`   | INSERT on tables Galexie does not touch                                | Lambda Ingestion                                       |
| `partition_admin`         | `partition_only` | `low_volume`   | `ALTER TABLE ... DROP PARTITION` + SELECT (admin-class, quota-bounded) | Lambda Partition mgmt                                  |
| `migration_admin`         | `migration_full` | `low_volume`   | DDL + read + write (one-shot migrations)                               | Lambda Migration                                       |
| `dict_reader` (existing)  | `read_only_lan`  | n/a (loopback) | SELECT inside container (loopback only)                                | Dictionary SOURCE clause                               |

**Auth split — two paths:**

- **External clients** (Lambdas, Galexie, dev laptops via mTLS) reach
  scoped users (or `dev_shared`) through Caddy proxy-trust:
  `<no_password/>` with `<networks>` restricted to the compose bridge
  subnet + loopback. Cert is the credential; Caddy sets
  `X-ClickHouse-User: <user>` from the CN→user map.
- **Host-side clients** (`db-clickhouse-init` sidecar, backup script,
  operator from SSH→docker exec) connect as `default` with the
  password from `CLICKHOUSE_PASSWORD`. The image entrypoint manages
  the `default` user (we do NOT override it in `services.xml`) — its
  entrypoint-generated `default-user.xml` restricts `<networks>` to
  loopback (`127.0.0.1`/`::1`). External clients cannot reach
  `default` because (a) the Caddy CN→user map has no entry pointing
  to it, and (b) the password is not available to Caddy.

Dev certs map to `dev_shared` (NOT `default`) so a cert alone is
enough for dev access without exposing the password-protected
`default` over the bridge. Per-dev attribution lives in Caddy
access logs (`X-Client-Subject` header forwards the full DN);
joining Caddy logs with CH `system.query_log` gives the "which
dev ran which query" answer.

The compose bridge subnet is pinned explicitly in
`docker-compose.prod.yml` (e.g. `172.30.0.0/16`) so the `<networks>`
allowlist in `services.xml` is stable and reviewable. Docker's
default subnet allocator picks unpredictable ranges, which would
make the allowlist either too narrow (broken after a network
rebuild) or too wide (`172.16.0.0/12` covering all Docker bridges).

### Caddy CN → CH user mapping (new responsibility)

Caddy gets a `map` directive translating the verified mTLS CN to a CH
user, plus a guard rejecting any cert whose CN does not appear in the
map:

```caddy
map {tls_client_subject_cn} {ch_user} {
    galexie-production            galexie
    lambda-api-production         api_reader
    lambda-ingestion-production   ingestion_writer
    lambda-partition-production   partition_admin
    lambda-migration-production   migration_admin
    <firstname>-laptop            dev_shared
    # additional per-developer CNs map to `dev_shared`, Ansible-rendered
    # unmatched CN → {ch_user} empty → 403 below
}

@no_user expression `{ch_user} == ""`
respond @no_user 403 {
    body "no clickhouse user mapping for this cert"
}

reverse_proxy localhost:8123 {
    header_up X-ClickHouse-User {ch_user}  # `set` already replaces client value
}
```

This **replaces** the existing CN allowlist snippet from 0227 — the
`map` IS the allowlist (any unmapped CN gets 403). One source of
truth; the old snippet is deleted in the same change.

### Profiles to define

Add to `crates/db-clickhouse/users.d/profiles.xml`:

- `admin` — `readonly=0`, `allow_ddl=1`, large memory + execution
  limits
- `read_only` — `readonly=1`, `max_memory_usage` capped,
  `max_execution_time` 30s
- `write_no_ddl` — `readonly=0`, `allow_ddl=0`, optimised for INSERT
  throughput (`max_insert_block_size`, `min_insert_block_size_rows`)
- `partition_only` — `readonly=0`, `allow_ddl=0`, plus a SQL-level
  grant for `ALTER TABLE ... DROP PARTITION` only
- `migration_full` — `readonly=0`, `allow_ddl=1`, time-limited
  execution (no hung migrations)
- `read_only_lan` — same as `read_only` but `networks` field limits
  to loopback (already in place for `dict_reader`)

Profiles must inherit timeout settings from the existing
`timeouts.xml` or duplicate them defensively — verify in Phase 1.

### Quotas to define

Add to `crates/db-clickhouse/users.d/quotas.xml`:

- `unlimited` — sidecar + dev laptops + emergency
- `api_throttle` — `10000 queries / hour`, `1B read_rows / hour`,
  `100 GB read_bytes / hour`, `1000s execution_time / hour`
- `high_write` — `unbounded queries`, `unbounded read`, `1 PB
written_bytes / hour` ceiling (sanity, not a real cap)
- `low_volume` — `100 queries / hour` (migration / partition jobs
  are infrequent)

### Files

```
crates/db-clickhouse/users.d/
├── dict.xml             (existing — dict_reader, no change)
├── timeouts.xml         (existing — verify new profiles inherit or copy timeouts in)
├── profiles.xml         (NEW — all profile definitions, static)
├── quotas.xml           (NEW — all quota definitions, static)
└── services.xml         (NEW — dev_shared + galexie + lambda-* users, <no_password/> + <networks>, static)
```

`services.xml` is fully static — no env-var rendering. It defines
the proxy-trust users (`dev_shared`, `galexie`, `api_reader`,
`ingestion_writer`, `partition_admin`, `migration_admin`). `default`
is NOT defined here — the image entrypoint manages it (password
from `CLICKHOUSE_PASSWORD`, loopback `<networks>` from the
entrypoint-generated `default-user.xml`).

### Ansible changes

- Template change: render the Caddy `map` directive (inline in
  `Caddyfile.j2`) from the existing CN list — service CNs map to
  their service user, dev CNs map to `default`. **Remove** the
  standalone `cn_allowlist.snippet.j2` and its include from
  `Caddyfile.j2` — the `map` subsumes its role (single source of
  truth for who is allowed in).
- Keep `CLICKHOUSE_PASSWORD` (host-side admin path: sidecar,
  backup, SSH→docker exec). External clients reach scoped users
  or `dev_shared` via Caddy without ever touching the password.
- Reload Caddy on CN-list change; restart CH container on
  `services.xml` change (existing handlers).
- **Not added (vs original plan)**: no `CLICKHOUSE_PASSWORD_<USER>`
  env-var preflight; no per-service password rendering; no
  coordination work with [[task-0239]] on Secrets-Manager-bundle
  shape; no `devs.xml` rendering.

### init.sql changes

**No changes** — verified empirically that CH 26.3 refuses SQL
`GRANT` / `REVOKE` against users defined in `users.d/*.xml` storage
(error: `ACCESS_STORAGE_READONLY`). The earlier draft of this task
planned to use init.sql to narrow `partition_admin` via SQL GRANTs;
this is not reachable for XML-defined users.

Per-user RBAC is therefore enforced **entirely at the profile
gate** (`readonly`, `allow_ddl`) plus quotas:

- `api_reader` (`readonly=1`) — only SELECT can run, so a SQL `GRANT
SELECT` would be redundant.
- `galexie` / `ingestion_writer` (`write_no_ddl`, `allow_ddl=0`) —
  INSERT + SELECT permitted, DDL blocked at the profile gate; a SQL
  `GRANT INSERT` would be redundant.
- `migration_admin` (`migration_full`, `allow_ddl=1`) — full DDL
  permitted, intentionally.
- `partition_admin` (`partition_only`, `allow_ddl=1`) — admin-class
  permissions bounded by quota (`low_volume`, 100 queries/h) and
  `max_execution_time` 300s. Documented as a deliberate trade-off
  (see Risks).

The sidecar `db-clickhouse-init` continues to run as `default`
(admin, password-less via the loopback override). It runs the same
`init.sql` as today — no GRANT-related work to add.

### Caddy mTLS → CH user mapping (audit trail)

| Caddy CN (verified by mTLS)      | Mapped CH user     |
| -------------------------------- | ------------------ |
| `galexie-<environment>`          | `galexie`          |
| `lambda-api-<environment>`       | `api_reader`       |
| `lambda-ingestion-<environment>` | `ingestion_writer` |
| `lambda-partition-<environment>` | `partition_admin`  |
| `lambda-migration-<environment>` | `migration_admin`  |
| `<firstname>-laptop`             | `dev_shared`       |

Caddy access logs already forward `X-Client-Subject` (full DN) +
`X-Client-Cert-Fingerprint`. ClickHouse access logs record
`currentUser()`. Joining the two streams gives full provenance: which
cert holder performed which query.

### Stacks / code affected (Lambda + Galexie side, in [[task-0239]])

This task is scoped to **CH-side + Caddy config only**. The Lambda /
Galexie code changes in [[task-0239]] Phase 2 stay minimal: configure
the CH HTTP client with `cert` + `key` + `ca` from the existing
Secrets Manager bundle. **No `ch_user` / `ch_password` reading
needed** — bundle keeps its `{cert, key, ca}` shape from 0227, no
Basic Auth header set on requests.

## Acceptance Criteria

- [x] **Phase 0 verifications V1, V2, V3 pass** on isolated CH 26.3 +
      Caddy 2.10 sandboxes. See history entries for empirical findings.
- [x] All five new profiles defined in
      `crates/db-clickhouse/users.d/profiles.xml` and applied by CH
      (verified via `SELECT name FROM system.settings_profile_elements`).
- [x] All four new quotas defined in
      `crates/db-clickhouse/users.d/quotas.xml` and applied (verified
      via `SELECT name, storage FROM system.quotas`).
- [x] Per-service users + `dev_shared` created with the right
      profile + quota binding (verified via `SELECT name, auth_type
FROM system.users`); all proxy-trust users have `<no_password/>` and
      `<networks>` restriction (`127.0.0.1` + `::1` + `172.30.0.0/16`).
      `default` remains image-entrypoint managed with password from
      `CLICKHOUSE_PASSWORD`.
- [x] `CLICKHOUSE_PASSWORD` env var preserved in compose stack +
      Ansible group_vars for the host-side admin path; sidecar +
      backup script + SSH→docker exec continue to use it.
- [x] Caddy `map` directive template added at
      `infra-hetzner/ansible/roles/app/templates/cn_user_map.snippet.j2`;
      Ansible task in `roles/app/tasks/main.yml` renders it from
      `clickhouse_cn_user_pairs` (parsed from env
      `CLICKHOUSE_CN_USER_MAP`). Standalone CN allowlist snippet from
      0227 removed (moved to `.trash/`). Live deploy is operator-driven
      and tracked separately as Phase 6 / [[task-0241]] cutover hooks.
- [x] Negative test: cert mapped to `api_reader` attempts `DROP TABLE`
      — got `Cannot execute query in readonly mode` (READONLY 164).
- [x] Negative test: cert mapped to `galexie` attempts `ALTER TABLE
... ADD COLUMN` — got `QUERY_IS_PROHIBITED` (392).
- [ ] ~~Negative test: cert mapped to `partition_admin` attempts
      `INSERT` — expect rejection.~~ **Deferred / superseded**:
      `partition_admin` profile must allow DDL for DROP PARTITION to
      work; CH XML user storage refuses SQL `REVOKE`/`GRANT` so the
      narrowing-via-init.sql plan is unreachable. Scope-narrowing
      delegated to quota (`low_volume`, 100 q/h) + max_execution_time
      300s; see Risks. Tighter scoping is a follow-up if business
      requirements change.
- [x] Negative test: cert with CN not in the `map` — Caddy returns
      403 via `@no_user` matcher (covered by Phase 0 V1 + by the
      "no CN in map → empty `{ch_user}`" mechanism in
      `cn_user_map.snippet.j2`).
- [x] Negative test (header spoofing): cert mapped to `api_reader` with
      client-supplied `X-ClickHouse-User: SPOOFED` — backend received
      only `api_reader` (Phase 0 V1: `header_up` in set-mode replaces).
- [x] Quota smoke: enforcement verified on host-side path
      (TCP-native / URL-param). Proxy-trust path (`X-ClickHouse-User`
      header) does NOT increment quota counters in CH 26.3.10 —
      documented as known limitation in spec Risks +
      `docs/architecture/security/clickhouse-rbac.md`. Follow-up
      investigation in [[task-0250]].
- [x] Sidecar `db-clickhouse-init` still runs `init.sql` as `default`
      with password from env; no new GRANT statements (CH XML
      user-storage is SQL-readonly — verified empirically in Phase 1).
      Sidecar verified working on dev compose (exit 0).
- [ ] ~~Host firewall (ufw / nftables) blocks port 8123 from
      non-loopback / non-Caddy sources~~ **Deferred to Phase 6
      operator deploy** (host-level firewall config is operator-time
      action on the live box, not part of the code change).
- [x] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [x] **Docs updated** —
      `docs/architecture/security/clickhouse-rbac.md` (NEW)
      documents the per-service user matrix, the Caddy CN → user
      mapping mechanism, the loopback `<no_password/>` model, and
      cert revocation procedure; `infra-hetzner/README.md`
      operating-model section adds a pointer to it;
      `docs/architecture/infrastructure/infrastructure-overview.md`
      gets a 2–3 sentence pointer to the new security doc in the CH
      section.

## Dependencies

- [[task-0227]] — Caddy mTLS gate + CN allowlist mechanism (✅
  delivered). This task **replaces** the CN allowlist snippet with
  the `map` directive — one source of truth instead of two
  (allowlist + future CH user table).
- [[task-0239]] — bundle shape stays `{cert, key, ca}` (no `ch_user`
  / `ch_password`), so this task has **no coordination requirement**
  with 0239 Phase 1's bundle work. 0239 can issue certs and
  configure Lambda HTTP clients independently. The only dependency
  direction: 0239 Phase 6 step 2 (drop RDS) requires this task done
  first.

## Risks / Considerations

- **Phase 0 V1 is load-bearing.** If Caddy `header_up -X-... +
header_up X-...` does not reliably strip every client-supplied
  instance of the header before setting the proxied value, any
  client with a valid cert can present as any CH user (including
  `migration_admin`) → full DDL bypass of RBAC. Cannot ship without
  this verified.
- **Host compromise = full CH access without further credentials.**
  Acceptable in our context (dedicated single-tenant box; host
  compromise already implies access to env files holding the
  password-design's passwords). Documented explicitly so future
  tenants / scope changes flag the assumption.
- **CH listen address drift.** An accidental
  `<listen_host>0.0.0.0</listen_host>` would expose `<no_password/>`
  users to the public internet, bypassing Caddy entirely. Mitigated
  by Ansible assertion on `config.d/listen.xml` plus the host
  firewall rule.
- **`partition_admin` is effectively admin, bounded by quota.**
  Verified empirically in Phase 1: CH 26.3 refuses SQL
  `GRANT` / `REVOKE` against XML-defined users
  (`ACCESS_STORAGE_READONLY`). The original "DROP PARTITION + SELECT
  only" intent cannot be expressed without moving partition_admin
  to SQL-managed user storage (`CREATE USER`), which would break
  the consistency of the XML-managed user model. Accepted
  trade-off: the `partition_only` profile (`allow_ddl=1`,
  `max_execution_time` 300s) + `low_volume` quota (100 queries/h)
  bound the blast radius — a compromised partition_admin Lambda can
  ALTER / DROP up to 100 times an hour, never longer than 5 min per
  query. Acceptable for an internal Lambda gated by AWS IAM. If a
  stricter posture is later required, spawn a follow-up to migrate
  partition_admin (only) to SQL-managed storage.
- **Quotas NOT enforced via proxy-trust path** (verified empirically
  in Phase 4 on CH 26.3.10). When a request reaches CH with
  `X-ClickHouse-User` header (the path Caddy uses) instead of URL
  param `?user=` or TCP-native auth, CH counts 0 queries against
  the quota — counter increments only for URL-param and TCP-native
  paths. CH 26.3 also explicitly rejects mixing header + URL param
  (`Invalid authentication: it is not allowed to use X-ClickHouse
HTTP headers and authentication via parameters simultaneously`),
  so the obvious "set both" workaround is blocked. Consequence:
  `api_throttle`, `low_volume`, `high_write` are effectively unused
  for Caddy-proxied traffic — DoS protection for that path lives in
  other layers (AWS API Gateway throttle for Lambda, Caddy's own
  `request_body max_size`, host firewall, profile
  `max_execution_time` + `max_memory_usage`). Quotas still
  enforce for the host-side path (sidecar, backup, SSH→docker
  exec as `default`). Defining the quotas in `quotas.xml` is
  retained because (a) host-side path uses them, (b) they
  document intent, (c) a future CH upgrade or Caddy URL-rewrite
  refactor could restore enforcement without a config rewrite.
- **`default` is password-protected, image-entrypoint managed.**
  Sidecar + backup script + SSH→docker exec use it with
  `CLICKHOUSE_PASSWORD` (Ansible-rendered into `/srv/app/.env`).
  We do NOT override `default` in `services.xml` — the image
  entrypoint generates `users.d/default-user.xml` with
  `<networks>` loopback (`127.0.0.1`/`::1`) only. External clients
  (Caddy-proxied) cannot reach `default` because no CN→user map
  entry points to it AND Caddy does not know the password to forge
  Basic Auth.
- **Dev certs = full admin via `dev_shared`.** Any developer with a
  valid mTLS cert mapped to `dev_shared` can `DROP TABLE` on
  production CH. Accepted trade-off: devs already need admin for
  debugging and ad-hoc migrations, and per-dev RBAC would have
  added a profile + quota + user + Ansible-rendered devs.xml per
  developer for limited operational gain. Per-dev attribution
  remains in Caddy logs (CN forwarded as `X-Client-Subject`).
  Cert revocation (remove CN from `CLICKHOUSE_CN_USER_MAP`,
  re-render Caddy map, reload) is the single-edit kill switch.
- **SSH access to the box still implies CH admin.** Operator with
  SSH key for `deploy` has sudo+docker group and can either
  `cat /srv/app/.env` (read the password) or `docker exec` to CH.
  Hasło chroni przed atakami "SSH klucz bez dostępu do `/srv/app/.env`"
  — w naszym setupie nie ma tego scenariusza (deploy user czyta env
  file). Hasło utrzymane głównie dla operacyjnej higieny i
  potencjalnego compliance, nie jako realna granica security w
  obecnej topologii.
- **CN revocation = single map edit.** Cert revoked = remove CN from
  the Ansible CN list, re-render Caddy `map`, reload. No password
  rotation, no Lambda redeploy. Same procedure for revoking a dev
  laptop as for revoking a service cert.
- **Cert rotation** still 365 days from `issue-client-cert.sh`. Same
  CN survives rotation, so the Caddy `map` entry stays valid — pure
  cert swap, no CH-side changes. Significantly simpler than the
  password-rotation procedure the original plan needed.

## Out of Scope

- SQL-RBAC migration away from XML `users.d/`. CH supports
  `CREATE ROLE` + `GRANT TO ROLE` since 20.10 — cleaner for
  team-of-N management, but the static XML approach is fine for
  ≤10 services. Spawn a separate refactor task if/when the
  user matrix grows beyond manageable XML.
- Per-table column-level grants (CH supports `GRANT SELECT
(column1, column2) ON table`). Not needed for the current
  service set; add if business requirements emerge.
- ClickHouse-side audit log shipping (currently `system.query_log`
  retains 30 days, queryable directly). Long-term log archival
  is its own observability task.
- Multi-tenant isolation (separate CH databases per tenant). Not
  applicable to our indexer use case.
