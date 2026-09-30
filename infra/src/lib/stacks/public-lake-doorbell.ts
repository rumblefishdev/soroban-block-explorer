import * as cdk from 'aws-cdk-lib';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as scheduler from 'aws-cdk-lib/aws-scheduler';
import * as targets from 'aws-cdk-lib/aws-scheduler-targets';
import type * as sqs from 'aws-cdk-lib/aws-sqs';
import { Construct } from 'constructs';

// Seconds between doorbells. Measured 2026-09-30 over an hour of testnet: a
// ledger file lands in the lake ~2.4 s (p50) after close, and each step below
// 2 s buys ~0.5 s of lag for twice the indexer calls (task 0553).
const DOORBELL_EVERY_SECONDS = 2;
// One schedule fires once a minute and sends one SendMessageBatch, which
// holds at most ten messages.
const MESSAGES_PER_BATCH = 10;

export interface PublicLakeDoorbellProps {
  readonly envName: string;
  /** The indexer's ingest queue; each message wakes one reconcile. */
  readonly ingestQueue: sqs.IQueue;
}

/**
 * Rings the indexer every `DOORBELL_EVERY_SECONDS` for an environment that
 * reads SDF's public data lake, which publishes no notifications (task 0553).
 *
 * EventBridge Scheduler fires at most once a minute, so each schedule sends a
 * batch of SQS messages with staggered `DelaySeconds`, and consecutive
 * schedules cover consecutive stretches of the minute. The indexer reads no
 * message content: every message only starts a reconcile from
 * `max(sequence) + 1`.
 */
export class PublicLakeDoorbell extends Construct {
  constructor(scope: Construct, id: string, props: PublicLakeDoorbellProps) {
    super(scope, id);

    const { envName, ingestQueue } = props;
    const delays: number[] = [];
    for (let s = 0; s < 60; s += DOORBELL_EVERY_SECONDS) delays.push(s);

    for (let b = 0; b * MESSAGES_PER_BATCH < delays.length; b++) {
      const batch = delays.slice(
        b * MESSAGES_PER_BATCH,
        (b + 1) * MESSAGES_PER_BATCH
      );
      new scheduler.Schedule(this, `Batch${b}`, {
        scheduleName: `${envName}-lake-doorbell-${b}`,
        description: `Doorbells at seconds ${batch.join(', ')} of every minute`,
        schedule: scheduler.ScheduleExpression.rate(cdk.Duration.minutes(1)),
        target: new targets.Universal({
          service: 'sqs',
          action: 'sendMessageBatch',
          input: scheduler.ScheduleTargetInput.fromObject({
            QueueUrl: ingestQueue.queueUrl,
            Entries: batch.map((delay) => ({
              Id: `s${delay}`,
              MessageBody: 'doorbell',
              DelaySeconds: delay,
            })),
          }),
          // SendMessageBatch is authorised by sqs:SendMessage.
          policyStatements: [
            new iam.PolicyStatement({
              actions: ['sqs:SendMessage'],
              resources: [ingestQueue.queueArn],
            }),
          ],
        }),
      });
    }
  }
}
