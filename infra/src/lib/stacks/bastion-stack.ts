import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ssm from 'aws-cdk-lib/aws-ssm';
import type { Construct } from 'constructs';

import { HTTPS_PORT, POSTGRESQL_PORT } from '../ports.js';

export interface BastionStackProps extends cdk.StackProps {
  readonly envName: string;
  readonly awsRegion: string;
  readonly vpcCidr: string;
}

/**
 * Bastion host for SSM Session Manager port forwarding to RDS.
 *
 * Runs as a separate CDK app so `deploy --all` on the main app
 * never touches the bastion. Deploy/destroy on-demand:
 *
 *   make deploy-staging-bastion
 *   make destroy-staging-bastion
 *
 * Looks up VPC and RDS SG from the main app via tags and SSM Parameters.
 * Cost: ~$3/mo when running, $0 when stack is destroyed.
 */
export class BastionStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: BastionStackProps) {
    super(scope, id, props);

    const { envName, vpcCidr } = props;

    // ---------------------
    // Lookups
    // ---------------------
    // VPC created by the main app's NetworkStack (tagged at stack level).
    const vpc = ec2.Vpc.fromLookup(this, 'Vpc', {
      tags: {
        Project: 'soroban-block-explorer',
        Environment: envName,
      },
    });

    // RDS security group ID exported as SSM Parameter by RdsStack.
    const rdsSgId = ssm.StringParameter.valueFromLookup(
      this,
      `/soroban-explorer/${envName}/rds-security-group-id`
    );
    const rdsSg = ec2.SecurityGroup.fromSecurityGroupId(this, 'RdsSg', rdsSgId);

    // ---------------------
    // Security Group
    // ---------------------
    // Zero inbound rules. SSM Session Manager does not need inbound access.
    // Egress: HTTPS for SSM agent communication, PostgreSQL for RDS tunnel.
    const bastionSg = new ec2.SecurityGroup(this, 'BastionSg', {
      vpc,
      description: 'Bastion host for SSM port forwarding',
      allowAllOutbound: false,
    });
    bastionSg.addEgressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.tcp(HTTPS_PORT),
      'Allow SSM agent to reach AWS SSM service'
    );
    bastionSg.addEgressRule(
      ec2.Peer.ipv4(vpcCidr),
      ec2.Port.tcp(POSTGRESQL_PORT),
      'Allow port forwarding to RDS on PostgreSQL port'
    );

    // Allow bastion to reach RDS.
    // Uses L1 CfnSecurityGroupIngress to avoid issues with imported SG.
    new ec2.CfnSecurityGroupIngress(this, 'BastionToRdsIngress', {
      groupId: rdsSg.securityGroupId,
      ipProtocol: 'tcp',
      fromPort: POSTGRESQL_PORT,
      toPort: POSTGRESQL_PORT,
      sourceSecurityGroupId: bastionSg.securityGroupId,
      description: 'Allow Bastion to RDS on PostgreSQL port',
    });

    // ---------------------
    // Bastion Host
    // ---------------------
    const bastion = new ec2.BastionHostLinux(this, 'Bastion', {
      vpc,
      subnetSelection: { subnetType: ec2.SubnetType.PUBLIC },
      instanceType: new ec2.InstanceType('t4g.nano'),
      securityGroup: bastionSg,
      instanceName: `${envName}-soroban-explorer-bastion`,
    });

    // ---------------------
    // SSM Parameter
    // ---------------------
    new ssm.StringParameter(this, 'BastionInstanceIdParam', {
      parameterName: `/soroban-explorer/${envName}/bastion-instance-id`,
      stringValue: bastion.instanceId,
      description: 'Bastion EC2 instance ID for SSM port forwarding',
    });

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    // ---------------------
    // Outputs
    // ---------------------
    new cdk.CfnOutput(this, 'BastionInstanceId', {
      value: bastion.instanceId,
    });
  }
}
