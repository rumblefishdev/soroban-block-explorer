# mTLS Certificate Authority

This directory holds the tooling for the self-signed Certificate
Authority that gates mTLS access to the production ClickHouse
endpoint. The CA is the **root of trust** for all cross-cloud
authentication; treat its private key with the same discipline
as a database admin password.

## What is in this directory

```
ca.crt                 Public CA certificate. Committed to the repo.
                       Mounted into Caddy as the trusted client-CA bundle.

generate-ca.sh         One-time CA bootstrap. Generates ca.crt + ca.key.
                       Refuses to overwrite an existing CA.

issue-client-cert.sh   Per-consumer client certificate issuance.
                       Signs a fresh client key with the CA key.

out/                   Issued client certificates land here.
                       Git-ignored; never commit private keys.
```

The CA **private** key never lives in this directory. It is
generated into a tmpfs (`/dev/shm/soroban-ca/ca.key`) and the
operator's responsibility is to move it to the team password
manager immediately after bootstrap.

## First-time CA bootstrap

Run this once per environment (production has one CA; if a
staging environment is ever added, it gets its own CA, never
shares).

```bash
cd infra-hetzner/ca/
./generate-ca.sh
```

The script prints next-step instructions on success. Summary:

1. Copy `/dev/shm/soroban-ca/ca.key` into the password-manager
   entry `soroban-prod / ca-key`.
2. Verify the entry round-trips correctly.
3. `shred -u /dev/shm/soroban-ca/ca.key && rmdir /dev/shm/soroban-ca`.
4. `git add infra-hetzner/ca/ca.crt && git commit`.

The CA certificate is valid for 10 years. Rotation before that
is a planned operation (see "CA rotation" below).

## Issuing a client certificate

For every consumer that needs to authenticate to the production
ClickHouse endpoint — every dev laptop, every AWS service, every
CI runner — issue a dedicated certificate. Never share certs
across consumers; revocation is per-CN.

```bash
# Fetch the CA private key from the password manager into tmpfs.
mkdir -p /dev/shm/soroban-ca
chmod 0700 /dev/shm/soroban-ca
# (paste the key contents from the password manager)
chmod 0600 /dev/shm/soroban-ca/ca.key

# Issue the certificate.
cd infra-hetzner/ca/
./issue-client-cert.sh <CN>

# Output lands in ./out/<CN>/.
# When done with this batch:
shred -u /dev/shm/soroban-ca/ca.key
rmdir /dev/shm/soroban-ca
```

### CN conventions

| Consumer kind | CN pattern           | Example           |
| ------------- | -------------------- | ----------------- |
| Dev laptop    | `<firstname>-laptop` | `alice-laptop`    |
| AWS service   | `<service>-<env>`    | `lambda-api-prod` |
| CI runner     | `ci-<purpose>`       | `ci-smoke`        |

The CN is what surfaces in Caddy access logs (`X-Client-CN`
forwarded header) and is the unit of revocation. Pick descriptive
names; future-you will read them in incident postmortems.

## Delivering certificates to consumers

### Dev laptop

```bash
# On the dev's own machine, after fetching from secure transfer.
mkdir -p ~/.certs
mv <CN>.crt ~/.certs/
mv <CN>.key ~/.certs/
chmod 600 ~/.certs/<CN>.key
```

The dev should also paste both file contents into a password-manager
entry of their own (`soroban-prod / dev-cert-<CN>`) as a recovery
backup if their laptop disk fails.

### AWS service (Lambda, Galexie)

The Lambda runtime reads the certificate from AWS Secrets Manager
via the Parameters and Secrets Lambda Extension. The expected
secret format is a JSON document with three string fields:

```json
{
  "cert": "-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----\n",
  "key": "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n",
  "ca": "-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----\n"
}
```

Build and upload. The whole flow runs inside `/dev/shm` (tmpfs)
so a crash between writing `bundle.json` and shredding it leaves
the plaintext PEM material in RAM only — never on persistent
disk:

```bash
# Stage in tmpfs.
mkdir -p /dev/shm/soroban-cert-upload
chmod 0700 /dev/shm/soroban-cert-upload
cp infra-hetzner/ca/out/<CN>/{<CN>.crt,<CN>.key,ca.crt} \
    /dev/shm/soroban-cert-upload/
cd /dev/shm/soroban-cert-upload/

# Assemble the JSON bundle. python is universally available and
# handles PEM newlines correctly without jq's quirks.
python3 - <<'PY' > bundle.json
import json, pathlib
cn = "<CN>"  # replace
d = pathlib.Path(".")
bundle = {
    "cert": (d / f"{cn}.crt").read_text(),
    "key":  (d / f"{cn}.key").read_text(),
    "ca":   (d / "ca.crt").read_text(),
}
print(json.dumps(bundle))
PY

# Upload to AWS Secrets Manager (CDK creates the secret stub
# downstream; this command updates the value).
aws secretsmanager put-secret-value \
    --secret-id "soroban/<CN>-mtls" \
    --secret-string "file://bundle.json"

# Wipe the tmpfs stage — AWS SM is now the source of truth.
cd /
shred -u /dev/shm/soroban-cert-upload/*
rmdir /dev/shm/soroban-cert-upload

# Also remove the persistent issuance output (the original
# ./out/<CN>/ directory) for AWS service certs. Keep it for
# dev-laptop certs that the developer needs locally.
rm -rf infra-hetzner/ca/out/<CN>/
```

The IAM policy granting the Lambda execution role
`secretsmanager:GetSecretValue` on that ARN is wired in the AWS
CDK app (`infra/src/`), out of scope for this directory.

## Rotation

### Per-client rotation (annual)

Client certificates carry a 365-day validity by default. Rotate
before expiry:

```bash
./issue-client-cert.sh <CN> --force
```

This overwrites the previous output and re-issues. The new cert
must then be redelivered through the same channel as the original
(AWS SM for AWS services, `~/.certs/` + password manager for devs).

The previous cert remains valid until its own expiry — Caddy
accepts any non-expired CA-signed cert. For immediate cut-over
revocation, see below.

### Revocation

There is no CRL / OCSP infrastructure (deliberately out of scope
for the team scale). Revocation is performed by removing the CN
from the Caddy allowlist:

1. Edit `infra-hetzner/ansible/group_vars/all.yml`, remove the
   target CN from `allowed_client_cns`.
2. `ansible-playbook ... --tags app` — re-renders the snippet,
   compose handler restarts Caddy. Effective within seconds.

For lost laptops:

- Remove the laptop CN as above (~5 minutes including playbook
  run).
- Issue a new cert for the same dev under a new CN
  (e.g. `alice-laptop-2`) to avoid a name collision in audit
  logs.
- Add the new CN to `allowed_client_cns` and re-run `--tags app`.

This is a proactive-allowlist model rather than an exclude
blocklist: a brand-new cert with a fresh CN is denied by default
until the CN is explicitly added. The allowlist does not protect
against a CA key compromise — an attacker with `ca.key` can mint
a cert under an _existing_ allowed CN — but it does shrink the
attack surface for the more common case of a leaked / lost
individual client cert.

### CA rotation

A CA-level rotation invalidates every existing client certificate
and is a planned, scheduled operation. Procedure:

1. Announce in the team channel; coordinate a maintenance window.
2. Move the existing `ca.crt` and `ca.key` aside:
   ```bash
   mv infra-hetzner/ca/ca.crt infra-hetzner/ca/ca.crt.old
   # Rename the password-manager entry: 'soroban-prod / ca-key-OLD'.
   ```
3. Run `./generate-ca.sh` to generate the new CA.
4. Re-issue **every** client certificate against the new CA.
5. Distribute new certs to all consumers (AWS SM, dev laptops).
6. Deploy the new `ca.crt` to Caddy:
   `ansible-playbook ... --tags mtls_ca,caddy`.
7. Verify a synthetic mTLS canary works.
8. After a soak period, delete the old CA entries.

The 10-year CA validity means this should happen on the order
of once a decade absent a compromise event.

## Compromise response

If `ca.key` is suspected leaked:

1. Treat the production endpoint as compromised — assume any
   client cert can be forged until rotation completes.
2. **Cut traffic immediately.** The naive `ufw deny 443/tcp`
   blocks only _new_ connections; established TCP sessions stay
   up for up to the kernel's TCP retransmit window. `docker
compose stop` is also not instant — it sends SIGTERM and
   grants Caddy a graceful-drain window. And `docker kill`
   alone is insufficient because Caddy's `restart:
unless-stopped` policy in `docker-compose.prod.yml` causes
   the Docker daemon to **auto-restart the container after
   SIGKILL** (the `unless-stopped` flag only suppresses
   restart after an explicit `docker stop`, not after a kill).

   The correct sequence, in this exact order:

   ```bash
   ssh deploy@ch-prod-01
   sudo -i

   # 1. Kernel-level packet drop on port 443. CRITICAL: must
   #    target the DOCKER-USER chain, NOT INPUT. Docker's
   #    port-publish creates DNAT rules in PREROUTING that
   #    forward incoming :443 traffic into the container via
   #    the FORWARD chain — bypassing INPUT entirely. A rule
   #    in INPUT would block nothing for containerised Caddy.
   #    Docker explicitly reserves DOCKER-USER as the chain
   #    operators can append/insert into before its own rules.
   iptables -I DOCKER-USER -p tcp --dport 443 -j DROP

   # 2. Disable the Docker restart policy so the daemon will not
   #    auto-bring-back Caddy after we kill it. Without this,
   #    `docker kill` immediately respawns.
   docker update --restart=no caddy

   # 3. Now kill Caddy — SIGKILL, no graceful drain.
   docker kill caddy

   # 4. Forcibly close any TCP sessions that were already
   #    established before step 1's iptables rule (the rule only
   #    blocks NEW packets matching the filter; in-flight long-
   #    poll connections survive). `ss -K` uses the kernel's
   #    SOCK_DESTROY API and is part of iproute2 which ships in
   #    every Ubuntu base install — no extra package needed.
   ss -K state established '( dport = :443 or sport = :443 )'
   ```

   Verify: `ss -tnp | grep :443` should return nothing. Caddy
   container should not be in the `docker ps` output. Test from
   outside with `curl https://<domain>` — should fail to
   connect (not 403, not handshake error — connection refused
   or timeout).

3. Execute the CA rotation procedure above with no maintenance
   window — production is already effectively down.
4. Postmortem in a separate document.
