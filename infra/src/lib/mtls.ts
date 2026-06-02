import * as cdk from 'aws-cdk-lib';

/**
 * AWS-managed Lambda layer ARN for the
 * "AWS Parameters and Secrets Lambda Extension" (ARM64 build).
 *
 * AWS publishes this layer in every region from a region-specific
 * account; the version number is bumped roughly quarterly. This map
 * captures the regions we deploy into. If a new region is added,
 * extend the map — do NOT fall back to a single fixed ARN, because
 * cross-region layer references fail at deploy time with a confusing
 * "layer not found" error.
 *
 * Versions captured 2026-05-21 from the AWS docs (last AWS update
 * 2026-05-14). Bump these manually when refreshing — alternative
 * approach is the SSM dynamic reference
 * `{{resolve:ssm:/aws/service/aws-parameters-and-secrets-lambda-extension/arm64/latest}}`
 * which auto-resolves to the latest version at deploy time. Manual
 * pin chosen here for reproducibility; revisit if the rotation
 * cadence becomes painful.
 *
 * Source list: https://docs.aws.amazon.com/systems-manager/latest/userguide/ps-integration-lambda-extensions.html
 */
const SECRETS_EXTENSION_LAYER_ARN_ARM64: Record<string, string> = {
  'eu-central-1':
    'arn:aws:lambda:eu-central-1:187925254637:layer:AWS-Parameters-and-Secrets-Lambda-Extension-Arm64:78',
  'us-east-1':
    'arn:aws:lambda:us-east-1:177933569100:layer:AWS-Parameters-and-Secrets-Lambda-Extension-Arm64:80',
};

/**
 * Return the AWS-managed Secrets Manager Lambda Extension layer ARN
 * for the given region (ARM64 build). Throws on unknown regions so
 * synth fails fast instead of CloudFormation failing mid-deploy.
 */
export function secretsManagerLayerArn(region: string): string {
  const arn = SECRETS_EXTENSION_LAYER_ARN_ARM64[region];
  if (!arn) {
    throw new Error(
      `No AWS Parameters and Secrets Lambda Extension layer ARN known for region "${region}". ` +
        `Add it to SECRETS_EXTENSION_LAYER_ARN_ARM64 in infra/src/lib/mtls.ts. ` +
        `See https://docs.aws.amazon.com/secretsmanager/latest/userguide/retrieving-secrets_lambda.html for the regional ARN list.`
    );
  }
  return arn;
}

/**
 * Build the wildcard-suffixed Secrets Manager ARN for a secret name.
 *
 * AWS Secrets Manager appends a random 6-char suffix to every secret
 * ARN (e.g. `…secret:soroban/production/mtls/lambda-api-production-aBcDeF`).
 * IAM grants must use a wildcard to match. Returns the ARN form
 * `arn:aws:secretsmanager:<region>:<account>:secret:<name>-*`.
 *
 * The account ID is resolved via `cdk.Stack.of(scope).account` so this
 * works in both per-account synth and assumed-role deploys.
 */
export function mtlsSecretArn(scope: cdk.Stack, secretName: string): string {
  // Reject IAM-meaningful wildcards in the secret name. The function builds
  // an ARN like `…:secret:${secretName}-*` for IAM grants; an unexpected `*`
  // or `?` inside `secretName` would silently widen the grant beyond the
  // intended single-secret scope.
  if (/[*?]/.test(secretName)) {
    throw new Error(
      `mtlsSecretArn: secretName must not contain '*' or '?' (got: "${secretName}"); ` +
        `those characters widen the IAM grant beyond a single secret.`
    );
  }
  const region = scope.region;
  const account = scope.account;
  return `arn:aws:secretsmanager:${region}:${account}:secret:${secretName}-*`;
}
