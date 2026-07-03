# Hetzner production deployment — `infra-hetzner/`

Infrastructure-as-code for the production ClickHouse deployment
on the Hetzner-hosted `ch-prod-01` dedicated server. Decisions
behind this layout are recorded in [task 0216](../lore/1-tasks/active/0216_RESEARCH_hetzner-clickhouse-deploy/README.md);
this directory delivers the runnable artefacts.

## Directory map

```
infra-hetzner/
├── README.md                ← this file
├── installimage.conf        ← Hetzner installimage template (reproducibility/DR)
├── Caddyfile                ← TLS + mTLS reverse proxy config
├── ansible/                 ← Idempotent provisioning playbook
└── ca/                      ← Self-signed mTLS Certificate Authority tooling

(at repo root)
docker-compose.prod.yml      ← Production overlay over the dev compose file

(in crates/db-clickhouse/)
config.d/memory.xml          ← Production memory tuning
config.d/prometheus.xml      ← Native metrics endpoint on loopback
users.d/dict.xml             ← `dict_reader` user (loopback-only)
```

## Operating model

- **Deploy** runs from a developer laptop. The Ansible playbook
  picks up from "fresh Ubuntu 24.04 with SSH reachable as root"
  and takes the box to "production stack running, mTLS-protected,
  backups configured".
- **No git on the box.** The `app` role rsyncs the operator's
  LOCAL repo checkout into `/srv/app`. The box never reaches
  github.com for code; the operator's working tree is the single
  source of truth for the deployed state. A `DEPLOYED_INFO` file
  inside `/srv/app/` records the local SHA, branch, dirty status,
  and operator identity at deploy time.
- **Clean tree enforced** by default — the play refuses to sync
  if `git status --porcelain` returns anything. Override with
  `-e force_dirty=true` for an ad-hoc smoke deploy (the
  `dirty: true` flag will land in `DEPLOYED_INFO` for the audit
  trail).
- **CI-driven deploy** is deliberately not yet wired — local-only
  iteration first, automation once the procedure has been
  validated several times by hand. Future work: a GitHub Actions
  workflow that runs `actions/checkout` and then the same
  `ansible-playbook` invocation, with secrets sourced from GH
  Secrets instead of the local shell environment. The sync model
  carries over unchanged — the workflow's checkout becomes the
  rsync source.
- **Idempotent**: the playbook is safe to re-run. State-based
  modules detect drift; re-runs without configuration changes are
  no-ops aside from a small Compose health check.

## Prerequisites

On the deploy operator's laptop:

| Tool           | Why                                                         | Floor  |
| -------------- | ----------------------------------------------------------- | ------ |
| `ansible-core` | playbook runner                                             | 2.16+  |
| `openssl`      | CA bootstrap + client cert issuance                         | 1.1.1+ |
| `git`, `ssh`   | repo + box reachability                                     | recent |
| Python 3       | json bundle assembly for AWS SM                             | 3.10+  |
| `aws` CLI      | upload AWS service certs to SM                              | v2     |
| `hcloud` (pip) | `hetzner.hcloud` Storage Box modules (`pip install hcloud`) | recent |

In the team password manager (entries named exactly as below):

| Entry                           | Contents                        |
| ------------------------------- | ------------------------------- |
| `soroban-prod / ca-key`         | The CA private key (PEM)        |
| `soroban-prod / ansible-env`    | Shell-sourceable env-var block  |
| `soroban-prod / dev-cert-<you>` | Personal mTLS cert + key backup |

The `ansible-env` entry contains a shell block of the form:

```bash
# Source this before running ansible-playbook.

# --- Required ---
export HCLOUD_ROBOT_USER="..."          # Hetzner Robot webservice user (#ws+...)
export HCLOUD_ROBOT_PASSWORD="..."      # Robot webservice password
export HCLOUD_TOKEN="..."               # Hetzner Cloud API token (Console → Security → API tokens, read+write) — manages the Borg Storage Box subaccount
export STORAGEBOX_ID="..."              # Numeric Cloud ID of the BX21 Storage Box (Console → Storage Boxes)
export STORAGEBOX_SUBACCOUNT_PASSWORD="..."  # Random 32-byte base64 — password set on the Borg subaccount (used for the SFTP key-install login, not by the cron)
export CLICKHOUSE_PASSWORD="..."        # CH `default` user password
export BORG_PASSPHRASE="..."            # Borg repokey-blake2 passphrase
export CH_DOMAIN="..."                  # Caddy site address (e.g. ch.sorobanscan.rumblefish.dev — matches `chDomainName` in the CDK env config)
export ACME_EMAIL="..."                 # Let's Encrypt account email
export CLICKHOUSE_CN_USER_MAP="..."     # Comma-separated `<cn>:<ch_user>` pairs — Caddy uses both as the mTLS allowlist AND as the identity it forwards to CH (e.g. `<firstname>-laptop:dev_shared,galexie-production:galexie,...`). See docs/architecture/security/clickhouse-rbac.md.

# OPERATOR_SSH_PUBKEYS — multi-line. One OpenSSH public key per
# line. The `users` role writes this verbatim to the deploy
# user's authorized_keys. Onboarding a new operator = append
# their pubkey here + re-run playbook.
export OPERATOR_SSH_PUBKEYS="$(cat <<'PUBKEYS'
ssh-ed25519 ...your-laptop-pubkey...
PUBKEYS
)"

# --- Optional overrides (sensible defaults applied if unset) ---
# STORAGEBOX_SSH_USER / STORAGEBOX_SSH_HOST are DISCOVERED from the
# Cloud API by the `storagebox` play and need not be set. Pin them
# only to override, or to enable a `--tags backup`-only re-run that
# skips discovery (use the u…-subN user / host the role printed).
# export STORAGEBOX_SSH_USER="u123456-sub1"
# export STORAGEBOX_SSH_HOST="u123456-sub1.your-storagebox.de"
# export STORAGEBOX_SUBACCOUNT_NAME="borg-backup-ch-prod-01"
# export STORAGEBOX_SUBACCOUNT_HOME="borg-ch-prod-01-repo"
# export STORAGEBOX_RUN_BACKUP_VALIDATION="true"  # set false to skip the first-run full backup
# export STORAGEBOX_SSH_PORT="23"
# export BORG_REPO_PATH="./backups/clickhouse"
# Backup cadence + retention (task 0236): WEEKLY backups, keep 4.
# Defaults below; override only for a different cadence/retention.
# export BORG_KEEP_DAILY="0"
# export BORG_KEEP_WEEKLY="4"
# export BORG_KEEP_MONTHLY="0"
# export BORG_CRON_HOUR="3"
# export BORG_CRON_MINUTE="30"
# export BORG_CRON_WEEKDAY="0"   # 0 = Sunday; set "*" to back up daily
# export HOST_TIMEZONE="Etc/UTC"
# export HETZNER_SERVER_NAME="ch-prod-01"
```

Each operator copies it to `~/.config/soroban-prod.env` (or wherever
they prefer) with `chmod 600` and sources it before deploys.

In the Hetzner Robot UI:

- The dedicated server `ch-prod-01` already ordered and online.
- The operator's personal SSH public key registered in the Robot
  UI for the rescue-system fallback.

In the Hetzner Cloud Console (Storage Boxes moved here from Robot
in 2025):

- A `BX21` Storage Box ordered in the same data centre. Note its
  numeric ID → `STORAGEBOX_ID`.
- A read+write API token (Security → API tokens) → `HCLOUD_TOKEN`.
- The Borg backup **subaccount is created by the playbook** (the
  `storagebox` play) — no manual subaccount or SSH-key step.

## First-time setup (per environment)

These run exactly once per environment (`production`). Subsequent
operators inherit the artefacts via the password manager and git
history.

### 1. Bootstrap the mTLS CA

```bash
cd infra-hetzner/ca/
./generate-ca.sh
# Follow the on-screen instructions:
#   - Move /dev/shm/soroban-ca/ca.key to password manager entry
#     'soroban-prod / ca-key'.
#   - shred the tmpfs copy.
#   - Commit infra-hetzner/ca/ca.crt.
```

### 2. Issue your first developer cert

```bash
# Fetch CA private key from password manager into tmpfs.
mkdir -p /dev/shm/soroban-ca && chmod 0700 /dev/shm/soroban-ca
# (paste the key contents from the password manager)
chmod 0600 /dev/shm/soroban-ca/ca.key

cd infra-hetzner/ca/
./issue-client-cert.sh <your-firstname>-laptop

# Move the result somewhere safe and remove from out/.
mkdir -p ~/.certs
cp out/<your-firstname>-laptop/*.crt ~/.certs/
cp out/<your-firstname>-laptop/*.key ~/.certs/
chmod 600 ~/.certs/*.key

# Shred the CA key from tmpfs.
shred -u /dev/shm/soroban-ca/ca.key && rmdir /dev/shm/soroban-ca
```

### 3. Prepare the Ansible inventory

```bash
cd infra-hetzner/ansible/
cp inventory.ini.example inventory.ini
# Edit inventory.ini: replace REPLACE_WITH_SERVER_IP with the IP
# from Hetzner Robot.

# Install the Ansible collections the playbook depends on.
ansible-galaxy collection install -r requirements.yml
```

### 4. Configure the storage box and domain values

These are NOT edited in `group_vars/all.yml` — `all.yml` reads
them via `lookup('env', '...')`. Set them in
`~/.config/soroban-prod.env` (the canonical block lives in the
`soroban-prod / ansible-env` password-manager entry, copied from
the template in the "Prerequisites" section above):

- `CH_DOMAIN` → real DNS name pointed at the server IP (matches
  `chDomainName` in the CDK env config — provisioned by
  `HetznerDnsStack`)
- `ACME_EMAIL` → real operator email (LE expiry warnings)
- `STORAGEBOX_ID` → numeric Cloud ID of the BX21 Storage Box
  (Console → Storage Boxes). `HCLOUD_TOKEN` →
  read+write Cloud API token. `STORAGEBOX_SUBACCOUNT_PASSWORD` →
  random 32-byte base64. The `storagebox` play creates/reconciles
  the Borg subaccount and discovers its `STORAGEBOX_SSH_USER` /
  `STORAGEBOX_SSH_HOST` automatically — you do **not** set those.
- `OPERATOR_SSH_PUBKEYS` → multi-line block of OpenSSH public
  keys; one per operator authorised to SSH the box

Source the env file (`source ~/.config/soroban-prod.env`) before
each `ansible-playbook` run. Adding a new operator = append their
pubkey to the env block in the password manager + each operator
re-fetches the entry locally.

### 5. Smoke-test SSH connectivity

```bash
source ~/.config/soroban-prod.env
cd infra-hetzner/ansible/
ansible -i inventory.ini all -m ping
# Expected: ch-prod-01 | SUCCESS => { "ping": "pong" }
```

## Routine deploy

Every change to anything under `infra-hetzner/` or to
`docker-compose.prod.yml` lands via this flow:

```bash
source ~/.config/soroban-prod.env
cd infra-hetzner/ansible/
ansible-playbook -i inventory.ini site.yml
```

`--check --diff` is supported for a dry run; `--tags` scopes a
re-run to a subset of roles. Common targeted commands:

```bash
# Re-render /srv/app/.env, reload Caddy, recreate containers
# whose env changed.
ansible-playbook ... --tags app

# Apply only OS-hardening changes (sshd, ufw, fail2ban).
ansible-playbook ... --tags security

# Apply only Hetzner Robot side-channel changes (firewall, rDNS).
ansible-playbook ... --tags hetzner

# Reconcile the Borg Storage Box subaccount + authorised_keys via
# the Cloud API, then run a one-off validation backup. Skip the
# validation backup with -e storagebox_run_backup_validation=false.
ansible-playbook ... --tags storagebox
```

> **Docker log rotation — one-time container recreate.** The CH log cap
> (`logging: json-file max-size=100m max-file=5` in `docker-compose.prod.yml`)
> only applies to containers **created after** it lands. A `--tags app` run
> recreates the `clickhouse` service when its compose definition changed, so
> the cap takes effect then. If you deployed the backup change via a narrower
> tag set, force it once explicitly (the running container keeps its old,
> unbounded log driver because `daemon.json` has `live-restore: true`):
>
> ```bash
> docker compose -f /srv/app/docker-compose.yml \
>                -f /srv/app/docker-compose.prod.yml up -d --force-recreate clickhouse
> # verify the cap is live:
> docker inspect -f '{{.HostConfig.LogConfig}}' clickhouse   # → max-size:100m max-file:5
> ```

## Post-deploy verification

After the first deploy and after any potentially disruptive
re-run:

```bash
ssh deploy@<ch-prod-01-ip>

# All three services should report healthy.
docker compose -f /srv/app/docker-compose.yml \
               -f /srv/app/docker-compose.prod.yml ps

# CH responds intra-host.
docker exec clickhouse clickhouse-client -q 'SELECT version()'

# CH responds via Caddy with a valid client cert.
exit
clickhouse-client --secure \
  -h "$CH_DOMAIN" \
  --user default --password "$CLICKHOUSE_PASSWORD" \
  --config-file ~/.clickhouse-client/config.xml \
  -q 'SELECT version()'

# Synthetic negative: connection without cert is rejected at TLS layer.
curl -sSI "https://${CH_DOMAIN}/ping"
#   → "alert handshake failure" (TLS abort), NOT 200.

# Synthetic negative: connection with a valid CA-signed cert
# whose CN is NOT in `CLICKHOUSE_CN_USER_MAP` gets 403 at the HTTP
# layer (the Caddy `map` IS the allowlist — unmapped CN yields an
# empty `{ch_user}` and the @no_user matcher returns 403 before
# any backend hop). Issue a throwaway cert for this test, then
# leave its CN out of the map — confirm that absence denies traffic.
```

## Adding a developer

Two env vars in the operator's `~/.config/soroban-prod.env` drive
this (no group_vars edit, no commit):

1. Append the new dev's SSH public key to `OPERATOR_SSH_PUBKEYS`
   (one OpenSSH key per line in the heredoc — see the
   group_vars/all.yml comment for the multi-line shape).
2. Append `<newdev>-laptop:dev_shared` to `CLICKHOUSE_CN_USER_MAP`
   — this both adds the cert to the mTLS allowlist (Caddy `map`)
   and tells Caddy which CH user to forward the request as
   (`dev_shared` — admin, no_password, loopback/bridge-restricted).
   See [`docs/architecture/security/clickhouse-rbac.md`](../docs/architecture/security/clickhouse-rbac.md)
   for the full user matrix.

Then:

3. Each operator re-fetches the password-manager entry into their
   `~/.config/soroban-prod.env`.
4. `ansible-playbook ... --tags users,caddy_reload`.
5. Issue the new dev's mTLS cert:
   ```bash
   ./infra-hetzner/ca/issue-client-cert.sh <newdev>-laptop
   ```
   Hand the cert bundle over via password manager.

## Removing a developer

1. Remove the dev's SSH key line from `OPERATOR_SSH_PUBKEYS` and
   the matching `<dev>-laptop:dev_shared` entry from
   `CLICKHOUSE_CN_USER_MAP` in `~/.config/soroban-prod.env`.
   Each operator re-fetches.
2. `ansible-playbook ... --tags users,caddy_reload`.
3. The `users` role rebuilds `authorized_keys` from scratch each
   run, so removing the line evicts SSH access. The Caddy `map`
   snippet is re-rendered from `CLICKHOUSE_CN_USER_MAP`, so
   removing the CN evicts mTLS access — effective on Caddy
   reload (triggered by the snippet-change handler).

If the offboarding is hostile (laptop unreturned), the CN
removal is the immediate access cut — no CA rotation needed, no
CRL/OCSP infrastructure required.

## Disaster recovery

### Box is unreachable (kernel panic / hardware fault)

1. From Hetzner Robot UI, reboot via management console. Wait
   for the box to come back; if it returns, `--tags app` to
   re-establish containers.
2. If a clean reboot fails: boot the rescue system from Robot UI.
   The box is on `mdadm` RAID 1 → either disk surviving is enough
   to mount and inspect.
3. If the data array is intact: replace the failed disk
   (Hetzner can do this; raise a support ticket), re-add the
   replacement to the array, let it rebuild.
4. If the data array is gone: see "Total loss" below.

### Total loss

> **Active-compromise variant**: if the box loss is the result
> of an active compromise (rooted, ransomware, suspected
> data-exfiltration), the FIRST step is to evict the dead box's
> Borg SSH public key from the Storage Box BEFORE provisioning
> the new box. Otherwise the attacker — who still has the dead
> box's `/root/.ssh/borg_ed25519` — can `borg delete` every
> archive while you set up the replacement. The dead key lives in
> the subaccount's `authorized_keys`, which is reachable over SSH
> only from inside Hetzner (`reachable_externally: false`) — you
> cannot reach it from your laptop. Cut access via the **Cloud
> API**, which needs no connection to the Storage Box: disable SSH
> on the Borg subaccount, or delete it. In the Hetzner Console:
> Storage Boxes → the box → Sub-accounts → turn SSH off (or delete
> the subaccount). Equivalently, with `HCLOUD_TOKEN`/`STORAGEBOX_ID`
> set, run `hetzner.hcloud.storage_box_subaccount` with
> `access_settings.ssh_enabled=false` (or `state: absent`). This
> instantly revokes the attacker's key-based access. Only then
> proceed with step 1; the full playbook run in step 4 recreates /
> re-enables the subaccount with only the new box's key authorised.

1. Order a new dedicated server in Hetzner Robot UI.
2. Apply `installimage.conf` from this directory via the Hetzner
   installimage tool. Reboots into fresh Ubuntu 24.04 on RAID 1.
3. Update `inventory.ini` with the new IP.
4. Run the full playbook, skipping the first-run validation backup
   (you are about to restore, not back up):

   ```bash
   ansible-playbook -i inventory.ini site.yml \
       -e storagebox_run_backup_validation=false
   ```

   This brings the stack up empty, generates a new Borg SSH keypair
   on the box, and — via the `storagebox` play — reconciles the
   Storage Box subaccount and **authorises the new box's pubkey on
   it automatically** (overwriting `authorized_keys`, which also
   revokes the dead box's key). No manual Robot UI / Console step.

5. Restore from the most recent Borg snapshot (FREEZE-based — task 0236).

   The archive contains: immutable MergeTree **parts**, the **live schema**
   (`_schema.sql` — `SHOW CREATE` of every table+dictionary), and a
   **uuid↔name map** (`_table_uuids.tsv`). You re-create the EXACT schema
   from `_schema.sql` (NOT `init.sql` — which can drift from prod via online
   ALTERs), then re-attach the parts. There is no SQL `RESTORE`.

   > Drill-tested locally end-to-end (mixed partitioned/plain/RMT schema,
   > full FREEZE→borg→extract→ATTACH roundtrip, fingerprints matched). A real
   > BX21 restore is still the operator's first LIVE exercise — rehearse it
   > once on a throwaway box.

   ```bash
   ssh deploy@<new-box-ip>
   SB="ssh://<sb-user>@<sb-host>:23/./backups/clickhouse"   # from `borg list`
   export BORG_PASSCOMMAND="cat /etc/soroban-backup/borg.passphrase"
   export BORG_RSH="ssh -i /root/.ssh/borg_ed25519"          # cron key; root has no default identity

   # a) pick the archive (prefer the most recent that did NOT end with
   #    warnings in `borg list`, if a clean later one exists)
   sudo -E borg list "$SB"

   # b) extract. The archive stores paths RELATIVE to the freeze root
   #    (`store/<uuid>/…`, `_schema.sql`, `_table_uuids.tsv`), so they land
   #    directly under the --target dir.
   sudo mkdir -p /tmp/borg-restore
   sudo -E borg extract --target /tmp/borg-restore "$SB::ch-<stamp>"
   SHADOW=/tmp/borg-restore

   CB="sudo docker compose -f /srv/app/docker-compose.yml -f /srv/app/docker-compose.prod.yml exec -T clickhouse"
   chq() { $CB clickhouse-client --config-file=/etc/clickhouse-backup/client.xml "$@" </dev/null; }

   # c) recreate the EXACT snapshot schema (replaces the init.sql schema the
   #    stack just created — so the recreated tables match the frozen parts
   #    even if prod had online ALTERs not in init.sql).
   for t in $(chq --query="SELECT name FROM system.tables WHERE database='default' AND engine!='Dictionary'"); do
     chq --query="DROP TABLE IF EXISTS default.\`$t\` SYNC"
   done
   for d in $(chq --query="SELECT name FROM system.dictionaries WHERE database='default'"); do
     chq --query="DROP DICTIONARY IF EXISTS default.\`$d\`"
   done
   # apply captured DDL (file on stdin — NOT chq, which redirects </dev/null)
   $CB clickhouse-client --config-file=/etc/clickhouse-backup/client.xml --multiquery < "$SHADOW/_schema.sql"

   # d) ATTACH each table's frozen parts.
   #    GOTCHAS: HOST path via the bind mount (data_paths[1] shows the
   #    container path); chown 101:101 after cp (else ATTACH EPERM);
   #    partitioned parts are `<pid>_*` not `all_*` → copy `*_*` and ATTACH
   #    PART per part; `chq </dev/null` avoids the docker-exec stdin-steal.
   #    A "NO PARTS" / "ATTACH FAILED" / "PARTIAL" line is a RED FLAG (table
   #    restores EMPTY or with missing rows) — investigate; do not ignore.
   miss=""
   while IFS=$'\t' read -r olduuid table; do
     [ -z "$table" ] && continue
     newuuid=$(chq --query="SELECT toString(uuid) FROM system.tables WHERE database='default' AND name='$table'" | tr -d '\r\n')
     [ -z "$newuuid" ] && { echo "!! MISSING TABLE $table (schema apply failed?)"; miss="$miss $table"; continue; }
     src="$SHADOW/store/${olduuid:0:3}/$olduuid"
     dst="/srv/clickhouse-data/store/${newuuid:0:3}/$newuuid/detached"
     sudo mkdir -p "$dst"
     if ! sudo cp -r "$src"/*_* "$dst"/ 2>/dev/null; then
       echo "!! NO PARTS for $table — investigate (empty table or path error)"; miss="$miss $table"; continue
     fi
     sudo chown -R 101:101 "$dst"
     # ATTACH every copied part; count successes vs parts present so a single
     # failed ATTACH (corrupt/version-skew part) can't masquerade as success.
     want=$(ls "$dst" | wc -l); got=0
     for part in $(ls "$dst"); do
       if chq --query="ALTER TABLE default.\`$table\` ATTACH PART '$part'"; then got=$((got+1))
       else echo "!! ATTACH FAILED: $table part $part"; fi
     done
     if [ "$got" -ne "$want" ]; then echo "!! PARTIAL: $table attached $got/$want parts"; miss="$miss $table"
     else echo "attached: $table ($got parts)"; fi
   done < "$SHADOW/_table_uuids.tsv"
   [ -n "$miss" ] && echo "!!!! NOT FULLY RESTORED:$miss — DO NOT declare success"

   # e) reload dictionaries (cache dicts loaded empty on the fresh stack;
   #    they only see the freshly-ATTACHed source rows after a reload).
   chq --query="SYSTEM RELOAD DICTIONARIES"

   # f) completeness + sanity. Every table from _table_uuids.tsv must appear
   #    with rows; RMT counts settle after a background merge (force if needed:
   #    OPTIMIZE TABLE default.<t> FINAL).
   chq --query="SELECT name, total_rows FROM system.tables WHERE database='default' ORDER BY name FORMAT TSV"

   # g) resume point. The marker (ledgers) was frozen FIRST → its max is the
   #    last FULLY-committed ledger; restart the indexer from max+1.
   chq --query="SELECT max(sequence)+1 AS resume_from FROM default.ledgers"
   ```

6. Validate the restore: row counts on a few large tables, schema
   discovery via `SELECT name FROM system.tables WHERE database = 'default'`.
7. Re-issue any mTLS service certs whose private keys were lost with
   the box (developer laptop certs are unaffected — those keys live
   on the laptops).
8. Re-point the prod DNS A-record at the new IP. Caddy obtains a new
   LE cert on first request; mind the LE rate limit (5 certs/week
   per domain) if you have already issued recently.
9. Smoke-test from a dev cert end-to-end before announcing
   restoration.

### Storage Box (BX21) is lost

Backups are off-site replicated only against operator-managed
backups (Borg → BX21 is the primary; no secondary). If BX21 is
lost, re-order, init a new Borg repo, accept the historical-
backup gap until the next weekly run lands.

### CA private key compromise

See `infra-hetzner/ca/README.md` §Compromise response.

## Operating notes

### Caddy / Let's Encrypt

- LE certificate state lives in the `caddy-data` Docker volume.
- The first deploy obtains a cert via http-01; Caddy renews
  automatically when within 30 days of expiry.
- DNS for `$CH_DOMAIN` must point at the server IP **before** the
  first deploy or the http-01 challenge fails. Caddy retries
  automatically once DNS is corrected. The record itself lives in
  Route 53 and is managed by the CDK `HetznerDnsStack` — see
  `infra/src/lib/stacks/hetzner-dns-stack.ts` and `make
deploy-production-hetzner-dns`.

### ClickHouse RBAC + auth model

Per-service users + profiles + quotas, plus the Caddy CN→user
mapping that drives proxy-trust identity, are documented in
[`docs/architecture/security/clickhouse-rbac.md`](../docs/architecture/security/clickhouse-rbac.md).
That file is the source of truth for the user matrix, profile /
quota definitions, the `CLICKHOUSE_CN_USER_MAP` env var format,
cert revocation procedure, and known limitations (notably that
quotas are NOT enforced for Caddy-proxied requests on CH 26.3 —
DoS protection lives in other layers).

### ClickHouse password rotation

The `default` ClickHouse user keeps a password (host-side admin
only: sidecar `db-clickhouse-init`, backup script, operator after
`ssh deploy && sudo`). External clients reach scoped users via
Caddy proxy-trust without ever touching this password — see the
RBAC doc above.

1. Update the `soroban-prod / ansible-env` entry in the password
   manager with the new value.
2. Each operator re-fetches the entry into their
   `~/.config/soroban-prod.env`.
3. `ansible-playbook ... --tags app` re-renders `/srv/app/.env`
   and recreates the CH container so the new password takes
   effect at the engine level.

### Borg passphrase rotation

**Two distinct scenarios — pick the right one. The wrong choice
in the wrong scenario leaves stolen archives still decryptable
by the attacker.**

#### Scenario A — scheduled rotation (no leak suspected)

Use `borg key change-passphrase`. This re-encrypts only the
key blob stored in the repo; historical archives stay
accessible to anyone who knows the new passphrase.

1. SSH to the box.
2. ```bash
   BORG_PASSCOMMAND="cat /etc/soroban-backup/borg.passphrase" \
   BORG_NEW_PASSCOMMAND="echo <new-passphrase>" \
   BORG_RSH="ssh -i /root/.ssh/borg_ed25519" \
   borg key change-passphrase "$BORG_REPO"
   ```
   (Without `BORG_NEW_PASSCOMMAND` the command prompts
   interactively, which does not work over `ansible exec`.)
3. Update the `soroban-prod / ansible-env` password-manager
   entry with the new value.
4. Re-deploy `--tags backup` to write the new passphrase to
   `/etc/soroban-backup/borg.passphrase`.

#### Scenario B — leak response (current passphrase compromised)

`change-passphrase` is **insufficient**. The repo on BX21
contains:

- Per-archive ciphertext encrypted with the data key.
- The data key, stored in the repo header, encrypted with
  the passphrase.

`change-passphrase` re-encrypts only the second item. Anyone
who already exfiltrated the repo and the old passphrase still
holds a copy of the data key — and can decrypt every historical
archive forever, regardless of subsequent passphrase changes.

To truly revoke historical access:

1. Cut the leak: rotate the SSH key the attacker may have used
   (`/root/.ssh/borg_ed25519`) — see "Adding a developer" /
   the backup role for re-issuance.
2. **Create a fresh Borg repo with a fresh passphrase.**
   `borg init` a new directory on BX21 (e.g. `./backups/ch-v2/`).
   The old repo still exists on BX21 but new backups land in
   the new one.
3. Update `borg_repo_path` in `group_vars/all.yml` to point at
   the new path; update the passphrase in the password manager
   and re-run `--tags backup`.
4. Hold the old repo for a short retention window (recovery
   from the period before the leak detection), then `borg
delete` it.
5. Accept the gap: archives from between leak-detection and
   new-repo cut-over are not yet protected from the leaked
   passphrase. There is no faster mitigation without
   reproducing the data from scratch (re-indexing the public
   Stellar chain).

`borg recreate` exists as a third option to re-encrypt all
archives in place with new key material, but takes time
proportional to repo size (hours on this dataset) and holds
the repo lock the whole time. Acceptable for a controlled
maintenance window; not for rapid incident response.

### Logs

| What           | Where                                               |
| -------------- | --------------------------------------------------- |
| Caddy access   | `docker logs caddy` (JSON, structured)              |
| CH server      | `docker logs clickhouse` + `/srv/clickhouse-logs/`  |
| Sidecar init   | `docker logs db-clickhouse-init`                    |
| Backup cron    | `/var/log/ch-backup.log` (rotated weekly, 26 weeks) |
| fail2ban / ufw | `journalctl -u fail2ban -u ufw`                     |

## Future work (out of scope for the current task)

- GitHub Actions workflow `.github/workflows/deploy-hetzner.yml`
  that wraps the same playbook with secrets sourced from GH
  Secrets. The playbook is environment-agnostic, so the workflow
  is a thin shell.
- CODEOWNERS rule on the workflow file so a hostile commit cannot
  silently exfiltrate the GH Secrets.
- AWS CDK changes in `infra/src/` to remove the NAT Gateway, move
  Lambdas out of the VPC, and move Galexie to a public subnet —
  tracked separately as part of the AWS-side cutover.
- Native Prometheus scraper on the box + a dashboard. The CH
  metrics endpoint is exposed on `127.0.0.1:9363` and waiting.
- Borg restore drill (separate operational task).
