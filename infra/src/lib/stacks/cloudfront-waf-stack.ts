import * as cdk from 'aws-cdk-lib';
import type { Construct } from 'constructs';

import { WafWebAcl } from '../constructs/waf-web-acl.js';
import type { EnvironmentConfig } from '../types.js';

export interface CloudFrontWafStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
}

/**
 * CLOUDFRONT-scoped WAF WebACL for the frontend distribution.
 *
 * MUST be deployed to `us-east-1`: AWS requires CLOUDFRONT-scope
 * WebACLs to live in us-east-1 (a regional stack rejects them with
 * "The scope is not valid., parameter: CLOUDFRONT"). The CloudFront
 * distribution itself stays in `DeliveryStack` (config.awsRegion) and
 * references `webAclArn` through CDK `crossRegionReferences`.
 *
 * The REGIONAL WebACL for API Gateway is separate and lives in
 * `ApiGatewayStack` (a single WebACL cannot serve both scopes).
 */
export class CloudFrontWafStack extends cdk.Stack {
  readonly webAclArn: string;

  constructor(scope: Construct, id: string, props: CloudFrontWafStackProps) {
    super(scope, id, props);

    const { config } = props;

    const waf = new WafWebAcl(this, 'Waf', {
      scope: 'CLOUDFRONT',
      name: `${config.envName}-soroban-explorer-cf`,
      rateLimit: config.cloudFrontWafRateLimit,
    });

    this.webAclArn = waf.webAclArn;

    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    new cdk.CfnOutput(this, 'CloudFrontWafWebAclArn', {
      value: this.webAclArn,
    });
  }
}
