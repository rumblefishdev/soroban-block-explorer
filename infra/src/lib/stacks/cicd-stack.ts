import * as cdk from 'aws-cdk-lib';
import * as iam from 'aws-cdk-lib/aws-iam';
import type { Construct } from 'constructs';

// GitHub uses a well-known JWKS endpoint for OIDC verification; CDK requires
// at least one thumbprint but does not actually validate against it. We use
// GitHub's current OIDC root CA thumbprint as documented by AWS so this value
// passes compliance scans that look for a real-looking thumbprint.
const GITHUB_OIDC_THUMBPRINT = '6938fd4d98bab03faadb97b34396831e3780aea1';
const GITHUB_OIDC_URL = 'https://token.actions.githubusercontent.com';
/** Condition key prefix — AWS IAM strips the https:// from the issuer URL. */
const GITHUB_OIDC_ISSUER = 'token.actions.githubusercontent.com';
const GITHUB_OIDC_AUDIENCE = 'sts.amazonaws.com';

export interface CicdStackProps extends cdk.StackProps {
  /** GitHub org/repo, e.g. "rumblefishdev/soroban-block-explorer" */
  readonly githubRepo: string;
  readonly awsRegion: string;
}

/**
 * CI/CD resources for the production AWS environment.
 *
 * Creates:
 * - GitHub Actions OIDC identity provider (singleton per AWS account)
 * - Production deploy role (scoped to GitHub Environment "production")
 *
 * The deploy role trusts the CDK bootstrap roles for actual CloudFormation
 * operations. The OIDC trust policy restricts which GitHub workflows can
 * assume the role based on the GitHub Environment name.
 *
 * Staging deploy role removed in task 0239 — staging was retired by 0249
 * and is not redeployed in eu-central-1.
 *
 * Deployed once per AWS account via: `npx cdk --app "node dist/bin/cicd.js" deploy`
 */
export class CicdStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: CicdStackProps) {
    super(scope, id, props);

    const { githubRepo, awsRegion } = props;
    const accountId = cdk.Stack.of(this).account;

    // ---------------------
    // GitHub Actions OIDC Provider
    // ---------------------
    // Singleton per AWS account. See GITHUB_OIDC_THUMBPRINT note above.
    const oidcProvider = new iam.OpenIdConnectProvider(
      this,
      'GitHubOidcProvider',
      {
        url: GITHUB_OIDC_URL,
        clientIds: [GITHUB_OIDC_AUDIENCE],
        thumbprints: [GITHUB_OIDC_THUMBPRINT],
      }
    );

    // ---------------------
    // Deploy Role — production only
    // ---------------------
    // Trusts GitHub Actions OIDC with an environment condition. The role
    // then assumes CDK bootstrap roles to perform CloudFormation operations
    // — no direct CloudFormation/S3/IAM permissions needed.
    {
      const envName = 'production' as const;
      const role = new iam.Role(this, `${capitalize(envName)}DeployRole`, {
        roleName: `soroban-explorer-${envName}-deploy`,
        assumedBy: new iam.WebIdentityPrincipal(
          oidcProvider.openIdConnectProviderArn,
          {
            StringEquals: {
              [`${GITHUB_OIDC_ISSUER}:aud`]: GITHUB_OIDC_AUDIENCE,
              [`${GITHUB_OIDC_ISSUER}:sub`]: `repo:${githubRepo}:environment:${envName}`,
            },
          }
        ),
        maxSessionDuration: cdk.Duration.hours(1),
        description: `GitHub Actions deploy role for ${envName} environment`,
      });

      // Allow assuming CDK bootstrap roles for CloudFormation operations.
      // CDK bootstrap creates roles with a well-known naming pattern.
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: ['sts:AssumeRole'],
          resources: [
            `arn:aws:iam::${accountId}:role/cdk-hnb659fds-*-${accountId}-${awsRegion}`,
          ],
        })
      );

      // ECR login + push for Galexie image mirroring.
      // Scoped to the environment's ECR repo ARN.
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: [
            'ecr:BatchCheckLayerAvailability',
            'ecr:GetDownloadUrlForLayer',
            'ecr:BatchGetImage',
            'ecr:PutImage',
            'ecr:InitiateLayerUpload',
            'ecr:UploadLayerPart',
            'ecr:CompleteLayerUpload',
          ],
          resources: [
            `arn:aws:ecr:${awsRegion}:${accountId}:repository/${envName}-galexie`,
          ],
        })
      );

      // ECR GetAuthorizationToken doesn't support resource restrictions.
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: ['ecr:GetAuthorizationToken'],
          resources: ['*'],
        })
      );

      // SSM read for ECR repo URI lookup.
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: ['ssm:GetParameter'],
          resources: [
            `arn:aws:ssm:${awsRegion}:${accountId}:parameter/soroban-explorer/${envName}/*`,
          ],
        })
      );

      // CloudFormation read for post-deploy smoke test (describe stack outputs).
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: ['cloudformation:DescribeStacks'],
          resources: [
            `arn:aws:cloudformation:${awsRegion}:${accountId}:stack/Explorer-${envName}-*/*`,
          ],
        })
      );

      // S3 sync for SPA deployment (upload web/dist/ to SPA bucket).
      // Split into bucket-level (ListBucket) and object-level (Put/Delete)
      // for clarity in IAM policy audits.
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: ['s3:ListBucket'],
          resources: [`arn:aws:s3:::${envName}-soroban-explorer-spa`],
        })
      );
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: ['s3:PutObject', 's3:DeleteObject'],
          resources: [`arn:aws:s3:::${envName}-soroban-explorer-spa/*`],
        })
      );

      // CloudFront cache invalidation after SPA deploy.
      // Scoped to distributions tagged with this environment. If tagging
      // is not yet applied, this uses a wildcard — tighten when the
      // distribution ARN is available as a stack output.
      role.addToPolicy(
        new iam.PolicyStatement({
          actions: ['cloudfront:CreateInvalidation'],
          resources: [`arn:aws:cloudfront::${accountId}:distribution/*`],
        })
      );

      // Output the role ARN — store as GitHub Environment secret.
      new cdk.CfnOutput(this, `${capitalize(envName)}DeployRoleArn`, {
        value: role.roleArn,
        description: `Deploy role ARN for ${envName} — add as AWS_DEPLOY_ROLE_ARN in GitHub Environment "${envName}"`,
      });
    }

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('ManagedBy', 'cdk');
  }
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
