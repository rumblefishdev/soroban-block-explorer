# ClickHouse Auth and RBAC

Per-service users, profiles, and quotas on the Hetzner ClickHouse
deployment. Layer 3 of the defense-in-depth model (Layer 1 = TLS,
Layer 2 = mTLS + CN allowlist at Caddy, Layer 3 = ClickHouse-side
identity + privileges).

Source files:

- `crates/db-clickhouse/users.d/profiles.xml` — profile definitions
- `crates/db-clickhouse/users.d/quotas.xml` — quota definitions
- `crates/db-clickhouse/users.d/services.xml` — per-service users
- `infra-hetzner/Caddyfile` — proxy-trust + identity assertion
- `infra-hetzner/ansible/roles/app/templates/cn_user_map.snippet.j2`
  — CN → CH user mapping (Ansible-rendered from
  `CLICKHOUSE_CN_USER_MAP` env var)

## Auth model: two paths

External and host-side clients reach ClickHouse via different paths
with different credential models.

### External clients (Caddy proxy-trust)

Lambdas, Galexie, dev laptops connect over the public internet
through Caddy at `:443`. Caddy:

1. Terminates TLS with a Let's Encrypt server certificate.
2. Verifies the client's mTLS cert chain against the team CA
   (`infra-hetzner/ca/ca.crt`).
3. Looks up the verified cert subject CN in the
   `CLICKHOUSE_CN_USER_MAP` → resolves a ClickHouse user name.
4. Rejects with 403 if the CN is not in the map.
5. Forwards the request to `clickhouse:8123` with
   `X-ClickHouse-User: <user>` set from the map. The `set`-mode
   `header_up` directive replaces any client-supplied header value,
   so spoofing `X-ClickHouse-User: migration_admin` from the client
   is overwritten before the request leaves Caddy.

ClickHouse-side, every proxy-trust user is `<no_password/>` with
`<networks>` restricted to:

- `127.0.0.1` / `::1` (loopback for `docker exec` from the host)
- `172.30.0.0/16` (the compose bridge subnet, pinned in
  `docker-compose.prod.yml` and `docker-compose.yml`)

The cert is the credential — Caddy verifies it, CH trusts the
forwarded user name on a `<no_password/>` user. No password lives
on disk or in environment for any proxy-trust user.

### Host-side clients (`default` with password)

The sidecar `db-clickhouse-init`, the backup script, and an
operator running `clickhouse-client` after `ssh deploy@box && sudo
-i` connect as `default`. `default` is image-entrypoint managed —
the entrypoint generates `users.d/default-user.xml` (loopback-only
`<networks>`) and sets `<password>` from the
`CLICKHOUSE_PASSWORD` env var (rendered by Ansible into
`/srv/app/.env` from operator-supplied `CLICKHOUSE_PASSWORD`).

We deliberately do NOT override `default` in `services.xml`. The
image entrypoint owns it; external clients cannot reach it because
(a) the Caddy CN map has no entry pointing to `default` and (b)
Caddy does not know the password to forge Basic Auth.

## Per-service user matrix

| CH user            | Profile          | Quota          | Permitted operations                                                       | Consumer                                               |
| ------------------ | ---------------- | -------------- | -------------------------------------------------------------------------- | ------------------------------------------------------ |
| `default`          | `default`        | `default`      | Everything (admin, password from `CLICKHOUSE_PASSWORD` env)                | `db-clickhouse-init` sidecar + backup script + SSH ops |
| `dev_shared`       | `admin`          | `unlimited`    | Everything (admin, `<no_password/>` + loopback/bridge networks)            | Dev laptops (one shared cert-gated user)               |
| `galexie`          | `write_no_ddl`   | `high_write`   | INSERT only on ingestion tables                                            | Galexie ECS task                                       |
| `api_reader`       | `read_only`      | `api_throttle` | SELECT on `default.*`                                                      | Lambda API (read-heavy)                                |
| `ingestion_writer` | `write_no_ddl`   | `high_write`   | INSERT on tables Galexie does not touch                                    | Lambda Ingestion                                       |
| `partition_admin`  | `partition_only` | `low_volume`   | Profile-allowed DDL bounded by quota (100 q/h) + `max_execution_time` 300s | Lambda Partition mgmt                                  |
| `migration_admin`  | `migration_full` | `low_volume`   | DDL + read + write (one-shot migrations)                                   | Lambda Migration                                       |
| `dict_reader`      | `read_only_lan`  | n/a (loopback) | SELECT inside container (loopback only)                                    | Dictionary SOURCE clause                               |

## Caddy CN → CH user mapping

The map is rendered by Ansible from the `CLICKHOUSE_CN_USER_MAP`
env var. Each entry is `<cn>:<ch_user>`; the operator maintains
the full list. Convention:

| Caddy CN (verified by mTLS)      | Mapped CH user     |
| -------------------------------- | ------------------ |
| `galexie-<environment>`          | `galexie`          |
| `lambda-api-<environment>`       | `api_reader`       |
| `lambda-ingestion-<environment>` | `ingestion_writer` |
| `lambda-partition-<environment>` | `partition_admin`  |
| `lambda-migration-<environment>` | `migration_admin`  |
| `<firstname>-laptop`             | `dev_shared`       |

Service certs (issued via `infra-hetzner/ca/issue-client-cert.sh`)
get a CN matching their AWS role; dev certs get the operator's
firstname laptop CN. The map IS the allowlist: any unmapped CN
yields empty `{ch_user}` in Caddy, which the `@no_user` matcher
returns as 403 before any backend hop.

## Profiles and quotas

### Profiles

- `admin` — `readonly=0`, `allow_ddl=1`, large memory + execution
  limits.
- `read_only` — `readonly=1`, 4 GiB memory cap, 30 s execution.
- `write_no_ddl` — `readonly=0`, `allow_ddl=0`, INSERT-tuned block
  sizes, 8 GiB memory cap.
- `partition_only` — `readonly=0`, `allow_ddl=1`, 2 GiB memory,
  300 s execution. See "partition_admin caveat" below.
- `migration_full` — `readonly=0`, `allow_ddl=1`, 900 s execution
  (no hung migrations).
- `read_only_lan` — `read_only` plus loopback `<networks>`
  restriction (used by `dict_reader`).

### Quotas

- `unlimited` — sidecar + dev laptops + emergency.
- `api_throttle` — 10000 queries / hour, 1B read_rows, 100 GB
  read_bytes, 1000 s execution_time.
- `high_write` — unbounded queries / read, 1 PB written_bytes
  ceiling (sanity cap, not a real throttle).
- `low_volume` — 100 queries / hour for partition / migration
  one-shot jobs.

### partition_admin caveat

The intent for `partition_admin` was "DROP PARTITION + SELECT
only". Granular CH-side scoping at this level is constrained by
the platform; `partition_admin` is therefore bound to the
`partition_only` profile + `low_volume` quota as the operative
guardrail. Tighter scoping (SQL-managed user storage) is tracked
as a follow-up if business requirements change. See task
0240 history for the implementation analysis.

## Known limitations

### Rate limiting on proxy-trust path

CH-side quotas in `quotas.xml` are enforced on the host-side
connection path (sidecar, backup, SSH→docker exec). Rate limiting
on the Caddy-proxied path is delegated to upstream and in-query
layers: AWS API Gateway throttle, profile `max_execution_time`,
profile `max_memory_usage`, and Caddy's request body cap. Quotas
remain defined for the host-side path and as a forward-compatible
hook should the proxy-trust enforcement story change. See task
0250 for the active investigation.

### Box-level admin access

The Hetzner box's admin posture is governed by the host's SSH
access controls (deploy user + sudo + docker group). See
`infra-hetzner/README.md` and the `security`/`users` Ansible
roles for the host hardening details.

## Rotation procedures

### `CLICKHOUSE_PASSWORD` (default user)

1. Update the `soroban-prod / ansible-env` entry in the password
   manager with the new value (random 32-byte base64).
2. Each operator re-fetches the entry into their
   `~/.config/soroban-prod.env`.
3. `ansible-playbook ... --tags app` re-renders `/srv/app/.env`,
   `/etc/clickhouse-backup/client.xml`, and recreates the CH
   container so the new password takes effect at the engine level.

### Cert revocation (any user)

1. Edit `CLICKHOUSE_CN_USER_MAP` in `~/.config/soroban-prod.env` —
   remove the line whose CN is being revoked.
2. `ansible-playbook ... --tags caddy_reload` re-renders the
   Caddy `cn_user_map.snippet` and reloads Caddy. The revoked CN
   resolves to empty `{ch_user}` → 403 at the Caddy layer; the
   request never reaches CH.

This is the proactive equivalent of the CRL/OCSP path documented
in `infra-hetzner/ca/README.md`. Losing a laptop or rotating a
service cert is a 1-line edit + a playbook re-run, not a CA
rotation.

### Adding a new service / dev cert

1. Issue the cert with `infra-hetzner/ca/issue-client-cert.sh
<cn>`.
2. Append `<cn>:<ch_user>` to `CLICKHOUSE_CN_USER_MAP` in
   `~/.config/soroban-prod.env`.
3. `ansible-playbook ... --tags caddy_reload` to render and reload.

If the chosen `<ch_user>` doesn't exist yet (new service class),
also add it to `crates/db-clickhouse/users.d/services.xml`,
`--tags app` to sync the file and restart CH.

## Audit trail

Every request leaves two correlated entries:

- Caddy access log (stdout, JSON via the redact filter — see
  `infra-hetzner/Caddyfile`) records `X-Client-Subject` (full DN
  including CN) and `X-Client-Cert-Fingerprint`.
- ClickHouse `system.query_log` records `user` (which equals the
  CH user the request authenticated as).

Joining the two on timestamp + request URI answers "which cert
holder ran which query". Per-dev attribution for `dev_shared`
users requires this join (since CH sees them all as `dev_shared`);
service certs are 1:1 mapped to a CH user, so CH log alone
suffices.
