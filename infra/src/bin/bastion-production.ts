#!/usr/bin/env node
import { createRequire } from 'node:module';

import { createBastionApp } from '../lib/bastion-app.js';

const require = createRequire(import.meta.url);
const params = require('../../envs/production.json') as {
  envName: 'staging' | 'production';
  awsRegion: string;
  vpcCidr: string;
};

createBastionApp(params);
