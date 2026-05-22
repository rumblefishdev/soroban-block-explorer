import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';

import { HTTPS_PORT, STELLAR_OVERLAY_PORT } from '../ports.js';

export interface NetworkStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
}

/**
 * Minimum VPC for the Soroban Block Explorer post-task-0239.
 *
 * Only purpose: provide a public subnet so Galexie (ECS Fargate)
 * gets a routable public IPv4 per task and can reach both the
 * Stellar peer overlay and the Hetzner-hosted ClickHouse endpoint
 * over the Internet Gateway.
 *
 * Lambdas are intentionally NOT attached to this VPC — they reach
 * Hetzner CH via the AWS-managed Lambda egress path (free, no NAT
 * GW). mTLS handles identity, so VPC isolation is unnecessary.
 *
 * No NAT Gateway, no private subnets, no Lambda security group,
 * no S3 Gateway endpoint (the endpoint was only useful for private-
 * subnet workloads).
 */
export class NetworkStack extends cdk.Stack {
  readonly vpc: ec2.IVpc;
  readonly ecsSecurityGroup: ec2.ISecurityGroup;

  constructor(scope: Construct, id: string, props: NetworkStackProps) {
    super(scope, id, props);

    const { config } = props;

    // ---------------------
    // VPC — public subnet only
    // ---------------------
    // Single AZ is sufficient: Galexie is a singleton writer
    // (only one task at a time) and Fargate auto-replaces across
    // AZs on capacity failures within the assigned AZ set.
    const vpc = new ec2.Vpc(this, 'Vpc', {
      ipAddresses: ec2.IpAddresses.cidr(config.vpcCidr),
      availabilityZones: [...config.availabilityZones],
      natGateways: 0,
      restrictDefaultSecurityGroup: true,
      subnetConfiguration: [
        {
          name: 'Public',
          subnetType: ec2.SubnetType.PUBLIC,
          cidrMask: 20,
        },
      ],
    });
    this.vpc = vpc;

    // ---------------------
    // ECS Security Group — Galexie
    // ---------------------
    const ecsSg = new ec2.SecurityGroup(this, 'EcsSg', {
      vpc,
      description: 'ECS Fargate tasks (Galexie ingestion) — public subnet',
      allowAllOutbound: false,
    });
    // HTTPS — ECR image pull, CloudWatch Logs, S3 ledger bucket,
    // Secrets Manager (cert bundle), and Hetzner CH on :443 (mTLS).
    ecsSg.addEgressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.tcp(HTTPS_PORT),
      'Allow ECS to HTTPS (ECR, CW Logs, S3, Secrets Manager, Hetzner CH mTLS)'
    );
    // Stellar overlay protocol — Galexie connects to peers on :11625.
    ecsSg.addEgressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.tcp(STELLAR_OVERLAY_PORT),
      'Allow ECS to Stellar peer network (overlay protocol)'
    );
    this.ecsSecurityGroup = ecsSg;

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    // ---------------------
    // Outputs
    // ---------------------
    new cdk.CfnOutput(this, 'VpcId', { value: vpc.vpcId });
    new cdk.CfnOutput(this, 'PublicSubnetIds', {
      value: vpc
        .selectSubnets({ subnetType: ec2.SubnetType.PUBLIC })
        .subnetIds.join(','),
    });
    new cdk.CfnOutput(this, 'EcsSecurityGroupId', {
      value: ecsSg.securityGroupId,
    });
  }
}
