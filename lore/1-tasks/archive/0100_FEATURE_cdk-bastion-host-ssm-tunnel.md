---
id: '0100'
title: 'CDK: Bastion host with SSM tunnel for RDS access'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0031', '0032']
tags: [priority-medium, effort-small, layer-infra]
milestone: 1
links: []
history:
  - date: 2026-04-02
    status: backlog
    who: stkrolikiewicz
    note: 'Task created. Need secure remote access to RDS for DBeaver/psql via SSM port forwarding.'
  - date: 2026-04-02
    status: active
    who: stkrolikiewicz
    note: 'Activated for implementation'
  - date: 2026-04-02
    status: completed
    who: stkrolikiewicz
    note: >
      Implemented bastion as separate CDK app. 7 new files, 2 modified.
      BastionStack with VPC/SG lookups, SSM tunnel script, Makefile targets.
      Tested end-to-end: deploy, tunnel, DBeaver connection, destroy.
      Key decision: separate CDK app over same-app with flags.
---

# CDK: Bastion host with SSM tunnel for RDS access

## Summary

Add a separate, removable CDK stack (`BastionStack`) with an EC2 bastion host for SSM Session Manager port forwarding to RDS. Includes a convenience script (`db:tunnel`) that resolves instance ID and RDS endpoint from SSM Parameters and opens the tunnel in one command. Default local port is 15432 to avoid conflict with local docker-compose PostgreSQL on 5432.

## Context

RDS PostgreSQL is in a private subnet (`publiclyAccessible: false`). There is currently no way to connect from a developer machine (e.g. DBeaver, psql) to the database. SSM Session Manager port forwarding is the recommended approach — no SSH keys, no open ports, no VPN required.

The stack must be easy to deploy and destroy independently so it can be removed when not needed (cost savings).

### Port mapping

Developers often run local PostgreSQL (docker-compose) and need simultaneous access to staging RDS:

| Connection                | Host        | Port                            |
| ------------------------- | ----------- | ------------------------------- |
| Local (docker-compose)    | `localhost` | `5432`                          |
| Staging/Prod (SSM tunnel) | `localhost` | `15432` (default, configurable) |

SSM port forwarding tunnels `localhost:15432` through the bastion to `RDS:5432`. Local and remote ports are independent.

## Implementation Plan

### Step 1: RdsStack — export SSM Parameters

Add SSM Parameters in RdsStack (persist independently of bastion lifecycle):

- `/soroban-explorer/<env>/rds-endpoint` — RDS or RDS Proxy endpoint
- `/soroban-explorer/<env>/rds-security-group-id` — RDS SG ID for bastion ingress

### Step 2: BastionStack (separate CDK app)

Create `infra/src/lib/stacks/bastion-stack.ts`:

- **Separate CDK app** — `deploy --all` on main app never touches bastion
- Looks up VPC by tags (`Project`, `Environment`) and RDS SG ID from SSM Parameter
- `BastionHostLinux` with `t4g.nano`, Amazon Linux 2023, in **public subnet**
- Bastion SG: **zero inbound rules**, egress 443 (SSM) + 5432 to VPC CIDR
- `CfnSecurityGroupIngress` to add bastion→RDS rule on the looked-up SG
- Export instance ID as SSM Parameter: `/soroban-explorer/<env>/bastion-instance-id`

### Step 3: Bastion app entrypoints

- `infra/src/lib/bastion-app.ts` — `createBastionApp()` creates only BastionStack
- `infra/src/bin/bastion-staging.ts` — reads config from `envs/staging.json`
- `infra/src/bin/bastion-production.ts` — reads config from `envs/production.json`

### Step 4: Makefile — bastion targets with separate --app

Main app keeps `deploy --all`. Bastion targets use `BASTION_*_APP`:

```makefile
BASTION_STAGING_APP := node dist/bin/bastion-staging.js

deploy-staging-bastion: build
	npx cdk --app "$(BASTION_STAGING_APP)" deploy --require-approval broadening

destroy-staging-bastion: build
	npx cdk --app "$(BASTION_STAGING_APP)" destroy --force
```

### Step 5: Tunnel script

Create `tools/scripts/db-tunnel.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ENV=${1:-staging}
LOCAL_PORT=${2:-15432}

INSTANCE_ID=$(aws ssm get-parameter --name "/soroban-explorer/$ENV/bastion-instance-id" --query Parameter.Value --output text 2>/dev/null) || {
  echo "Error: Bastion not deployed for env '$ENV'." >&2
  echo "  Run: make deploy-$ENV-bastion" >&2
  exit 1
}

RDS_ENDPOINT=$(aws ssm get-parameter --name "/soroban-explorer/$ENV/rds-endpoint" --query Parameter.Value --output text 2>/dev/null) || {
  echo "Error: RDS endpoint not found for env '$ENV'." >&2
  exit 1
}

# Check instance is running
STATE=$(aws ec2 describe-instances --instance-ids "$INSTANCE_ID" --query 'Reservations[0].Instances[0].State.Name' --output text 2>/dev/null)
if [ "$STATE" != "running" ]; then
  echo "Error: Bastion instance $INSTANCE_ID is '$STATE', expected 'running'." >&2
  echo "  Start it: aws ec2 start-instances --instance-ids $INSTANCE_ID" >&2
  exit 1
fi

echo "Tunneling localhost:$LOCAL_PORT → $RDS_ENDPOINT:5432 via $INSTANCE_ID"

aws ssm start-session \
  --target "$INSTANCE_ID" \
  --document-name AWS-StartPortForwardingSessionToRemoteHost \
  --parameters "{\"host\":[\"$RDS_ENDPOINT\"],\"portNumber\":[\"5432\"],\"localPortNumber\":[\"$LOCAL_PORT\"]}"
```

Add `"db:tunnel": "./tools/scripts/db-tunnel.sh"` to root `package.json`.

## Acceptance Criteria

- [x] `BastionStack` runs as a separate CDK app (not in main app's `deploy --all`)
- [x] Bastion uses SSM Session Manager (no SSH, no public ports)
- [x] Bastion SG has zero inbound rules, egress only to RDS port (5432) and HTTPS (443 for SSM agent)
- [x] RDS SG gets ingress rule from bastion SG (removed when bastion stack is destroyed)
- [x] Instance ID exported as SSM Parameter from BastionStack
- [x] RDS endpoint exported as SSM Parameter from RdsStack
- [x] Makefile `deploy-staging`/`deploy-production` keep `--all` (bastion is separate app)
- [x] Makefile has `deploy-*-bastion` and `destroy-*-bastion` targets with separate `--app`
- [x] `npm run db:tunnel` opens port forwarding tunnel on localhost:15432
- [x] `npm run db:tunnel -- staging 5432` allows custom local port
- [x] Script validates: SSM params exist, instance is running
- [x] `cdk destroy Explorer-staging-Bastion` cleanly removes the bastion without affecting other stacks

## Implementation Notes

### Files created

| File                                    | Purpose                                                      |
| --------------------------------------- | ------------------------------------------------------------ |
| `infra/src/lib/stacks/bastion-stack.ts` | BastionHostLinux, SG, CfnSecurityGroupIngress, SSM Parameter |
| `infra/src/lib/bastion-app.ts`          | `createBastionApp()` — separate CDK app entrypoint           |
| `infra/src/bin/bastion-staging.ts`      | Bastion app entry — staging (reads envs/staging.json)        |
| `infra/src/bin/bastion-production.ts`   | Bastion app entry — production (reads envs/production.json)  |
| `tools/scripts/db-tunnel.sh`            | SSM tunnel script with validation                            |

### Files modified

| File                                | Change                                                       |
| ----------------------------------- | ------------------------------------------------------------ |
| `infra/src/lib/stacks/rds-stack.ts` | Added SSM Parameters: rds-endpoint, rds-security-group-id    |
| `infra/Makefile`                    | Added BASTION\_\*\_APP vars, deploy/destroy bastion targets  |
| `infra/README.md`                   | Added "Connecting to RDS" section, updated project structure |
| `package.json`                      | Added `db:tunnel` script                                     |

## Issues Encountered

- **Cross-stack cyclic dependency**: Initial approach passed `rdsSecurityGroup` as prop and called `addIngressRule()` — caused CDK cycle (BastionStack→RdsStack via addDependency + RdsStack→BastionStack via SG reference). Fix: used L1 `CfnSecurityGroupIngress` which doesn't create cross-stack references.

- **Separate app VPC lookup**: `Vpc.fromLookup` and `valueFromLookup` require the main app to be deployed first (VPC must exist, SSM Parameters must exist). Synth fails with clear error if prerequisites are missing. This is by design.

## Design Decisions

### From Plan

1. **SSM Session Manager, no SSH**: Zero inbound ports, no key management. SSM agent communicates outbound on HTTPS 443.

2. **Default local port 15432**: Avoids conflict with docker-compose PostgreSQL on 5432. Both connections work simultaneously.

3. **SSM Parameters for cross-app communication**: RDS endpoint and SG ID exported from RdsStack as SSM Parameters. Bastion app reads them via `valueFromLookup`. Decoupled from main app's stack outputs.

### Emerged

4. **Separate CDK app instead of same-app with config flag**: Initially planned `bastion: boolean` in EnvironmentConfig. Then tried always-synthesized in same app (required explicit stack lists in Makefile to avoid deploy with `--all`). Finally chose separate CDK app — cleanest isolation, `deploy --all` on main app never touches bastion, no flags needed. Trade-off: VPC/SG lookups instead of props (few extra lines).

5. **CfnSecurityGroupIngress (L1) instead of addIngressRule (L2)**: L2 `addIngressRule` on an imported SG from another stack created cyclic references. L1 `CfnSecurityGroupIngress` is a standalone CloudFormation resource owned by BastionStack — no cycle, clean destroy.

6. **Bastion SG needs egress 443 for SSM**: Original plan only mentioned egress 5432 to RDS. SSM agent on the bastion needs outbound HTTPS to communicate with AWS SSM service. Added egress 443 to 0.0.0.0/0.

7. **Entrypoints read from env JSON**: Initially hardcoded `awsRegion` and `vpcCidr` in bastion entrypoints. Production has different vpcCidr (`10.1.0.0/16` vs `10.0.0.0/16`). Changed to read from `envs/*.json` for consistency.

8. **Tunnel script in `tools/scripts/`**: Initially created `scripts/db-tunnel.sh` at repo root. Moved to `tools/scripts/` to match existing convention (`format-staged.mjs`, `generate-lore-board.mjs`, etc.).

## Notes

- Bastion in public subnet avoids needing SSM VPC Interface Endpoints (~$7/mo each for 3 endpoints).
- `t4g.nano` ARM instance costs ~$3/mo. Destroy stack when not needed for zero cost.
- SSM requires `session-manager-plugin` installed locally: `brew install session-manager-plugin`.
- RDS endpoint SSM Parameter lives in RdsStack so it persists independently of bastion lifecycle.
- Bastion is a separate CDK app — main app's `deploy --all` never touches it.
- BastionStack looks up VPC (by tags) and RDS SG (by SSM Parameter) — requires main app to be deployed first.
- RDS SG ID exported as SSM Parameter from RdsStack for cross-app lookup.
