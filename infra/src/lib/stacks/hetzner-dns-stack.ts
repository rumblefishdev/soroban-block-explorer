import * as cdk from 'aws-cdk-lib';
import * as route53 from 'aws-cdk-lib/aws-route53';
import * as ssm from 'aws-cdk-lib/aws-ssm';
import type { Construct } from 'constructs';

import { relativeRecordName, type EnvironmentConfig } from '../types.js';

export interface HetznerDnsStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
}

/**
 * Route 53 A record pointing the ClickHouse host name at the
 * Hetzner dedicated server's public IPv4. Standalone stack so it
 * can be deployed independently of the AWS-internal stacks
 * (frontend, api, compute) — needed before Caddy on the box can
 * pass the Let's Encrypt HTTP-01 challenge.
 *
 * Target is a literal IPv4 (not an AWS alias) because the box is
 * non-AWS (Hetzner Falkenstein). TTL is short (5 min) so an IP
 * change after a box replacement propagates fast.
 *
 * The IP itself is read from SSM Parameter Store at
 * `/soroban/${envName}/ch-ip` rather than from the env-config JSON.
 * This keeps the box IP out of git (matching the existing
 * `inventory.ini` gitignore convention) and lets operators rotate
 * the IP after a box replacement with a single `aws ssm
 * put-parameter` call plus `make deploy-${env}-hetzner-dns`, with
 * no PR / code review.
 *
 * Bootstrap (once per environment, before first deploy):
 *
 *     aws ssm put-parameter \
 *         --name /soroban/production/ch-ip \
 *         --value <dedicated-server-ipv4> \
 *         --type String \
 *         --region <awsRegion>
 *
 * `valueForStringParameter` renders a CFN dynamic reference
 * (`{{resolve:ssm:NAME:version}}`) — CloudFormation resolves it at
 * deploy time, so no AWS auth is required during `cdk synth` and
 * the IP never lands in the synthesized template stored locally.
 */
export class HetznerDnsStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: HetznerDnsStackProps) {
    super(scope, id, props);

    const { config } = props;

    const hostedZone = route53.HostedZone.fromHostedZoneAttributes(
      this,
      'HostedZone',
      {
        hostedZoneId: config.hostedZoneId,
        zoneName: config.hostedZoneName,
      }
    );

    const chIp = ssm.StringParameter.valueForStringParameter(
      this,
      `/soroban/${config.envName}/ch-ip`
    );

    const recordName = relativeRecordName(
      config.chDomainName,
      config.hostedZoneName
    );

    new route53.ARecord(this, 'ChARecord', {
      zone: hostedZone,
      recordName,
      target: route53.RecordTarget.fromValues(chIp),
      ttl: cdk.Duration.minutes(5),
      comment: `Non-AWS target — Hetzner dedicated server (${config.envName})`,
    });

    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    new cdk.CfnOutput(this, 'ChDomainName', { value: config.chDomainName });
  }
}
