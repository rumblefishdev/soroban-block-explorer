#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';

import { CiCdStack } from '../lib/stacks/cicd-stack.js';

const app = new cdk.App();

const env: cdk.Environment = {
  account: process.env['CDK_DEFAULT_ACCOUNT'],
  region: process.env['CDK_DEFAULT_REGION'] ?? 'us-east-1',
};

new CiCdStack(app, 'Explorer-CiCd', { env });

app.synth();
