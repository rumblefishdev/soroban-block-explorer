import * as cdk from 'aws-cdk-lib';
import * as iam from 'aws-cdk-lib/aws-iam';
import type { Construct } from 'constructs';

const GITHUB_ORG = 'rumblefishdev';
const GITHUB_REPO = 'soroban-block-explorer';

// GitHub Actions OIDC thumbprint for token.actions.githubusercontent.com.
// This is stable and published by GitHub:
// https://github.blog/changelog/2023-06-27-github-actions-update-on-oidc-integration-with-aws/
const GITHUB_OIDC_THUMBPRINT = '6938fd4d98bab03faadb97b34396831e3780aea1';

export type CiCdStackProps = cdk.StackProps;

/**
 * Account-level CI/CD stack.
 *
 * Creates the GitHub Actions OIDC identity provider (singleton per AWS account)
 * and least-privilege deploy roles for staging and production environments.
 *
 * Deploy roles use the CDK bootstrap role pattern: the GitHub Actions role itself
 * only has `sts:AssumeRole` on CDK bootstrap roles (`cdk-hnb659fds-*`).
 * All actual deploy permissions are scoped inside those bootstrap roles, which
 * are created by `cdk bootstrap` and are separate from application stacks.
 *
 * Deploy once per account — not per environment:
 *   make deploy-cicd
 */
export class CiCdStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: CiCdStackProps) {
    super(scope, id, props);

    // ---------------------
    // GitHub Actions OIDC Provider
    // ---------------------
    // One OIDC provider per IdP URL per AWS account — shared by both
    // staging and production deploy roles.
    const oidcProvider = new iam.OpenIdConnectProvider(
      this,
      'GithubActionsOidc',
      {
        url: 'https://token.actions.githubusercontent.com',
        clientIds: ['sts.amazonaws.com'],
        thumbprints: [GITHUB_OIDC_THUMBPRINT],
      }
    );

    // ---------------------
    // Deploy roles (one per environment)
    // ---------------------
    const stagingRole = this.createDeployRole(
      'StagingDeployRole',
      oidcProvider,
      'develop',
      'staging'
    );

    const productionRole = this.createDeployRole(
      'ProductionDeployRole',
      oidcProvider,
      'master',
      'production'
    );

    // ---------------------
    // Outputs — used by GitHub Actions workflow
    // ---------------------
    new cdk.CfnOutput(this, 'StagingDeployRoleArn', {
      value: stagingRole.roleArn,
      description: 'ARN of the GitHub Actions deploy role for staging',
      exportName: 'Explorer-staging-GithubActionsDeployRoleArn',
    });

    new cdk.CfnOutput(this, 'ProductionDeployRoleArn', {
      value: productionRole.roleArn,
      description: 'ARN of the GitHub Actions deploy role for production',
      exportName: 'Explorer-production-GithubActionsDeployRoleArn',
    });

    new cdk.CfnOutput(this, 'OidcProviderArn', {
      value: oidcProvider.openIdConnectProviderArn,
      description: 'ARN of the GitHub Actions OIDC provider',
    });

    // Tags
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('ManagedBy', 'cdk');
  }

  /**
   * Creates a GitHub Actions deploy role scoped to a specific branch.
   *
   * Permissions: only `sts:AssumeRole` on CDK bootstrap roles.
   * CDK bootstrap roles (created by `cdk bootstrap`) handle all actual
   * deploy permissions without granting AdministratorAccess to GitHub Actions.
   */
  private createDeployRole(
    id: string,
    oidcProvider: iam.IOpenIdConnectProvider,
    branch: string,
    envName: string
  ): iam.Role {
    const role = new iam.Role(this, id, {
      roleName: `Explorer-${envName}-GithubActionsDeployRole`,
      description: `GitHub Actions deploy role for ${envName} — assumes CDK bootstrap roles`,
      assumedBy: new iam.WebIdentityPrincipal(
        oidcProvider.openIdConnectProviderArn,
        {
          StringEquals: {
            'token.actions.githubusercontent.com:aud': 'sts.amazonaws.com',
          },
          StringLike: {
            'token.actions.githubusercontent.com:sub': `repo:${GITHUB_ORG}/${GITHUB_REPO}:ref:refs/heads/${branch}`,
          },
        }
      ),
    });

    // Least-privilege: only allow assuming CDK bootstrap roles.
    // This covers: deploy role, file-publishing role, image-publishing role,
    // and lookup role — all created by `cdk bootstrap`.
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'AllowCdkBootstrapRoleAssumption',
        effect: iam.Effect.ALLOW,
        actions: ['sts:AssumeRole'],
        resources: [`arn:aws:iam::${this.account}:role/cdk-hnb659fds-*`],
      })
    );

    return role;
  }
}
