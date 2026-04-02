import * as cdk from 'aws-cdk-lib';

import { BastionStack } from './stacks/bastion-stack.js';

export interface CreateBastionAppOptions {
  readonly envName: 'staging' | 'production';
  readonly awsRegion: string;
  readonly vpcCidr: string;
}

export function createBastionApp({
  envName,
  awsRegion,
  vpcCidr,
}: CreateBastionAppOptions): void {
  const app = new cdk.App();

  const env: cdk.Environment = {
    account: process.env['CDK_DEFAULT_ACCOUNT'],
    region: awsRegion,
  };

  new BastionStack(app, `Explorer-${envName}-Bastion`, {
    env,
    envName,
    awsRegion,
    vpcCidr,
  });

  app.synth();
}
