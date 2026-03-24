---
id: '0001'
title: 'Research: Galexie configuration, Captive Core setup, and output format'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0058', '0063']
tags: [priority-high, effort-medium, layer-research]
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: filip
    note: 'Task created from architecture docs decomposition'
---

# Research: Galexie configuration, Captive Core setup, and output format

## Summary

Investigate the configuration, deployment, and operational characteristics of Galexie running on ECS Fargate with Captive Core for live ledger export and historical backfill. This research must produce enough detail to write CDK task definitions, configure Captive Core networking, and validate the S3 output format contract that the downstream Ledger Processor Lambda depends on.

## Status: Backlog

## Context

The indexing pipeline architecture places Galexie as the first component in the data flow chain. It is responsible for converting live Stellar network peer connections into durable, replayable LedgerCloseMeta XDR artifacts stored in S3. The entire downstream pipeline (Ledger Processor Lambda, RDS PostgreSQL writes, API reads) depends on Galexie producing correct, timely, and consistently formatted output.

### Live Ingestion Path

Galexie runs on ECS Fargate as a continuously running task. It connects to Stellar network peers via Captive Core and exports one LedgerCloseMeta XDR file per ledger close. The expected cadence is approximately one file every 5-6 seconds, aligned with the Stellar ledger close interval. Galexie is checkpoint-aware and resumes from the last exported ledger on restart, which is a core reliability assumption of the pipeline.

### S3 Output Format

The output path follows the convention: `stellar-ledger-data/ledgers/{seq_start}-{seq_end}.xdr.zstd`. Files are zstd-compressed XDR, one LedgerCloseMeta per file. S3 PutObject events on this bucket trigger the Ledger Processor Lambda for event-driven parsing and persistence. This artifact contract is shared between live ingestion and historical backfill -- both write the same format to the same bucket.

### Historical Backfill Path

A separate ECS Fargate task handles historical backfill by reading Stellar public history archives. It produces the same LedgerCloseMeta XDR artifacts in the same S3 bucket and format as the live path, ensuring the same Ledger Processor Lambda handles both without any divergence in processing logic. Backfill scope starts from Soroban mainnet activation in late 2023 (approximately ledger 50,692,993). Backfill runs in configurable ledger-range batches with parallel non-overlapping ranges permitted.

### ECS Fargate Task Requirements

The Galexie ECS Fargate task definition requires specific CPU and memory allocations. The task must run inside a VPC with an S3 VPC endpoint for writing ledger artifacts without traversing the public internet. A NAT Gateway is needed for outbound connectivity to Stellar network peers, since Captive Core must reach external Stellar peer nodes.

### Pipeline Trigger Mechanism

S3 PutObject events on the `stellar-ledger-data` bucket trigger the Ledger Processor Lambda. This means the entire pipeline is event-driven from the moment Galexie writes a file -- there is no polling or scheduled check for new ledger data.

## Research Questions

- What are the Galexie CLI flags and configuration options for live export mode (output bucket, ledger range, compression format, checkpoint behavior)?
- What container image is required for Galexie? Is there an official Docker image, or must one be built from source?
- What Captive Core configuration is needed, including the network passphrase per environment (mainnet vs testnet)?
- What are the minimum CPU and memory requirements for the ECS Fargate task definition running Galexie with Captive Core?
- How does Galexie handle Captive Core peer connection failures and reconnection?
- What is the exact S3 key format and how are `seq_start` and `seq_end` determined (single ledger per file, or batched)?
- How does checkpoint-aware restart work -- does Galexie read its own S3 prefix to determine the last exported ledger, or does it use a separate state store?
- For the backfill task, what CLI flags control the ledger range, parallelism constraints, and history archive URL?
- What is the exact Soroban mainnet activation ledger number for scoping backfill start?
- How should the backfill task coordinate with live ingestion to avoid duplicate or conflicting writes to the same ledger sequence?

## Acceptance Criteria

- [ ] Documented Galexie CLI flags and configuration options for both live and backfill modes
- [ ] Container image source and build instructions (or official image reference)
- [ ] Captive Core configuration template with network passphrase for mainnet and testnet
- [ ] ECS Fargate task definition sizing recommendations (CPU, memory, storage)
- [ ] VPC networking requirements documented: S3 VPC endpoint, NAT Gateway for peer connectivity
- [ ] S3 output format contract validated: key pattern, compression, single vs batched ledgers
- [ ] Checkpoint restart behavior confirmed and documented
- [ ] Backfill execution model documented: ledger range configuration, parallelism, history archive URL
- [ ] Confirmed Soroban mainnet activation ledger number for backfill scoping

## Notes

- The infrastructure overview specifies a single-AZ launch topology in us-east-1a. Galexie networking must work within this constraint.
- The design explicitly requires open-source redeployability -- no hard-coded account IDs or internal-only dependencies.
- Failed ledger files remain in S3 and can be replayed by re-triggering the Lambda, so understanding the exact S3 key format is critical for operational recovery.
