import * as cdk from 'aws-cdk-lib';
import * as scheduler from 'aws-cdk-lib/aws-scheduler';
import * as targets from 'aws-cdk-lib/aws-scheduler-targets';
import type * as sqs from 'aws-cdk-lib/aws-sqs';
import { Construct } from 'constructs';

export interface PublicLakeKeepaliveProps {
  readonly envName: string;
  /** The indexer's ingest queue; each message wakes one reconcile. */
  readonly ingestQueue: sqs.IQueue;
  /**
   * False while the indexer is paused: a paused indexer gets no messages, so
   * none pile up in its queue.
   */
  readonly enabled: boolean;
}

/**
 * Wakes the indexer once a minute for an environment that reads SDF's public
 * data lake, which publishes no notifications (task 0553).
 *
 * The indexer paces itself, one delayed message per ledger it expects
 * (`crates/indexer/src/handler/lake_pacing.rs`). This keepalive starts that
 * chain, and restarts it when it has died — a failed message, a lake outage.
 */
export class PublicLakeKeepalive extends Construct {
  constructor(scope: Construct, id: string, props: PublicLakeKeepaliveProps) {
    super(scope, id);

    new scheduler.Schedule(this, 'Schedule', {
      scheduleName: `${props.envName}-lake-keepalive`,
      description: 'Wakes the indexer once a minute (task 0553)',
      enabled: props.enabled,
      schedule: scheduler.ScheduleExpression.rate(cdk.Duration.minutes(1)),
      target: new targets.SqsSendMessage(props.ingestQueue, {
        input: scheduler.ScheduleTargetInput.fromObject({ keepalive: true }),
      }),
    });
  }
}
