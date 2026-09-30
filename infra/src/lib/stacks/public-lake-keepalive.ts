import * as cdk from 'aws-cdk-lib';
import * as scheduler from 'aws-cdk-lib/aws-scheduler';
import * as targets from 'aws-cdk-lib/aws-scheduler-targets';
import type * as sqs from 'aws-cdk-lib/aws-sqs';
import { Construct } from 'constructs';

export interface PublicLakeKeepaliveProps {
  readonly envName: string;
  /** The indexer's ingest queue; each message wakes one reconcile. */
  readonly ingestQueue: sqs.IQueue;
}

/**
 * Wakes the indexer once a minute for an environment that reads SDF's public
 * data lake, which publishes no notifications (task 0553).
 *
 * A minute is the finest EventBridge Scheduler goes, and it fires anywhere
 * inside that minute, so this cannot pace the indexer ledger by ledger: the
 * indexer paces itself, sending one delayed message per ledger it expects.
 * This keepalive only restarts that chain when it has died — a failed
 * message, a lake outage — and wakes the indexer until the chain exists.
 */
export class PublicLakeKeepalive extends Construct {
  constructor(scope: Construct, id: string, props: PublicLakeKeepaliveProps) {
    super(scope, id);

    new scheduler.Schedule(this, 'Schedule', {
      scheduleName: `${props.envName}-lake-keepalive`,
      description: 'Wakes the indexer once a minute (task 0553)',
      schedule: scheduler.ScheduleExpression.rate(cdk.Duration.minutes(1)),
      target: new targets.SqsSendMessage(props.ingestQueue, {
        input: scheduler.ScheduleTargetInput.fromObject({ keepalive: true }),
      }),
    });
  }
}
