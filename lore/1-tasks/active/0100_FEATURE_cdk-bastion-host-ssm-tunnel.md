---
id: '0100'
title: 'CDK: Bastion host with SSM tunnel for RDS access'
type: FEATURE
status: active
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

### Step 1: EnvironmentConfig flag

Add `bastion: boolean` to `EnvironmentConfig` in `types.ts`. Set `true` in `staging.json`, `false` in `production.json`.

### Step 2: RdsStack — export endpoint as SSM Parameter

Add SSM Parameter `/soroban-explorer/<env>/rds-endpoint` in RdsStack (not BastionStack). This way the endpoint survives bastion stack destruction and is available to other tools.

### Step 3: BastionStack

Create `infra/src/lib/stacks/bastion-stack.ts`:

- `BastionHostLinux` with `t4g.nano`, Amazon Linux 2023, in **public subnet** (SSM works without VPC Endpoint)
- Bastion SG: **zero inbound rules**, egress only port 5432 to VPC CIDR
- Accept `rdsSecurityGroup` from RdsStack props, add ingress rule allowing bastion SG on port 5432
- Export instance ID as SSM Parameter: `/soroban-explorer/<env>/bastion-instance-id`

### Step 4: Wire into app.ts

Conditionally create BastionStack when `config.bastion` is `true`:

```typescript
if (config.bastion) {
  const bastion = new BastionStack(app, `${prefix}-Bastion`, {
    env,
    config,
    vpc: network.vpc,
    rdsSecurityGroup: rds.rdsSecurityGroup,
    rdsEndpoint: dbProxyEndpoint,
  });
  bastion.addDependency(rds);
}
```

No other stacks depend on BastionStack — `cdk destroy Explorer-staging-Bastion` removes it cleanly.

### Step 5: Tunnel script

Create `scripts/db-tunnel.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ENV=${1:-staging}
LOCAL_PORT=${2:-15432}

INSTANCE_ID=$(aws ssm get-parameter --name "/soroban-explorer/$ENV/bastion-instance-id" --query Parameter.Value --output text 2>/dev/null) || {
  echo "Error: Bastion not deployed for env '$ENV'. Deploy with: cdk deploy Explorer-$ENV-Bastion" >&2
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
  exit 1
fi

echo "Tunneling localhost:$LOCAL_PORT → $RDS_ENDPOINT:5432 via $INSTANCE_ID"

aws ssm start-session \
  --target "$INSTANCE_ID" \
  --document-name AWS-StartPortForwardingSessionToRemoteHost \
  --parameters "{\"host\":[\"$RDS_ENDPOINT\"],\"portNumber\":[\"5432\"],\"localPortNumber\":[\"$LOCAL_PORT\"]}"
```

Add `"db:tunnel": "./scripts/db-tunnel.sh"` to root `package.json`.

## Acceptance Criteria

- [ ] `BastionStack` is a separate CDK stack, conditionally created based on `config.bastion`
- [ ] Bastion uses SSM Session Manager (no SSH, no public ports)
- [ ] Bastion SG has zero inbound rules, egress only to RDS port
- [ ] RDS SG gets ingress rule from bastion SG (removed when bastion stack is destroyed)
- [ ] Instance ID exported as SSM Parameter from BastionStack
- [ ] RDS endpoint exported as SSM Parameter from RdsStack
- [ ] `npm run db:tunnel` opens port forwarding tunnel on localhost:15432
- [ ] `npm run db:tunnel -- staging 5432` allows custom local port
- [ ] Script validates: SSM params exist, instance is running
- [ ] `cdk destroy Explorer-staging-Bastion` cleanly removes the bastion without affecting other stacks
- [ ] Production config has `bastion: false` by default

## Notes

- Bastion in public subnet avoids needing SSM VPC Interface Endpoints (~$7/mo each for 3 endpoints).
- `t4g.nano` ARM instance costs ~$3/mo. Stack can be destroyed when not in use for zero cost.
- SSM requires `session-manager-plugin` installed locally: `brew install session-manager-plugin`.
- RDS endpoint SSM Parameter lives in RdsStack so it persists independently of bastion lifecycle.
