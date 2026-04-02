#!/usr/bin/env bash
set -euo pipefail

ENV=${1:-staging}
LOCAL_PORT=${2:-15432}

INSTANCE_ID=$(aws ssm get-parameter --name "/soroban-explorer/$ENV/bastion-instance-id" --query Parameter.Value --output text 2>/dev/null) || {
  echo "Error: Bastion not deployed for env '$ENV'." >&2
  echo "  Run: make -C infra deploy-$ENV-bastion" >&2
  exit 1
}

RDS_ENDPOINT=$(aws ssm get-parameter --name "/soroban-explorer/$ENV/rds-endpoint" --query Parameter.Value --output text 2>/dev/null) || {
  echo "Error: RDS endpoint not found for env '$ENV'." >&2
  exit 1
}

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
