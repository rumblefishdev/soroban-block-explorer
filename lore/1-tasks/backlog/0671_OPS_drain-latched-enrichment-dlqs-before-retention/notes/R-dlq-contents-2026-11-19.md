---
title: 'What the enrichment DLQs held on 2026-11-19, and what the testnet redrive showed'
type: research
status: mature
spawns: []
tags: ['enrichment', 'sqs', 'dlq', 'monitoring']
links:
  - docs/runbooks/dlq.md
  - crates/backfill-enrichment-runner/src/main.rs
  - crates/enrichment-shared/src/nft_token_uri/client.rs
history:
  - date: 2026-11-19
    status: mature
    who: stkrolikiewicz
    note: 'Every message read before the testnet redrive; the redrive and its errors'
  - date: 2026-11-24
    status: mature
    who: stkrolikiewicz
    note: 'Production read again: 8 messages arrived after 11-19 without a page'
---

# What the enrichment DLQs held on 2026-11-19, and what the testnet redrive showed

Step 1 and part of step 2 of the task. All times +01:00.

## How it was read

Every message was received with `VisibilityTimeout=0` (no message hidden, none deleted) around
12:30, before any redrive. That raised each `ApproximateReceiveCount` from 3 to 4. A message
received more than three times drops out of `ApproximateAgeOfOldestMessage`, so after this read
that metric understates the age; `SentTimestamp` below is the real one.

## What was there

| Queue      | Messages         | Kind                          | Sent                       | Where it failed (worker log)                               |
| ---------- | ---------------- | ----------------------------- | -------------------------- | ---------------------------------------------------------- |
| testnet    | 39 (26 distinct) | `sep1_assets`, 3 issuers      | 11-05 14:21                | `stellar.toml` 5xx, the 11-05 redrive                      |
| testnet    | 4                | `nft_token_uri`, 4 contracts  | 11-05 14:21                | IPFS, the 11-05 redrive                                    |
| testnet    | 3                | `nft_token_uri`               | 11-17 ~09:00               | `gateway.pinata.cloud`: error sending request, 08:18–08:36 |
| production | 2                | `nft_token_uri`, one contract | 11-16 ~22:00, 11-17 ~20:00 | `gateway.pinata.cloud`, 18:16 and 19:37–19:43 on 11-17     |

The 16 production messages from 11-05 had aged out before this read; their class can't be
attributed any more.

## The testnet redrive (13:27)

`aws sqs start-message-move-task` on `testnet-enrichment-dlq`: 46 of 46 moved. 2 landed, 44 came
back at 13:36 after three tries each. The worker's ERROR lines from 13:25:

| Error                                                                           | Lines | Messages         |
| ------------------------------------------------------------------------------- | ----- | ---------------- |
| `HTTP error fetching stellar.toml from dev.radox.net: HTTP status server error` | 117   | 39 `sep1_assets` |
| `metadata fetch (gateway.pinata.cloud): error sending request`                  | 9     | 3 NFT            |
| `metadata fetch (ipfs.io): HTTP status client error (429 Too Many Requests)`    | 9     | 3 NFT            |

`testnet-enrichment-dlq-depth` went OK at 13:29 and back to ALARM with the 44. The messages
that came back carry a new `SentTimestamp` (11-19 13:27:05), so they age out around 12-03.

## What it shows

- **The testnet worker predates 0520's gateway fix.** It still fetches through `ipfs.io`, which
  `DEFAULT_IPFS_GATEWAYS` on `develop` no longer lists (`d38ed8a1`, `d26df881`: Filebase, then
  Pinata, in pool order). Its function has no `IPFS_GATEWAY_BASES` and was last deployed
  2026-11-06 13:13 UTC. The production worker (2026-11-12 13:37 UTC, another `CodeSha256`) logs
  only Pinata, the last gateway in that order. A redrive on testnet can't test 0520's step 3
  until its worker is deployed from `develop`.
- **`dev.radox.net` answers 5xx for all three issuers.** A 5xx is transient, so these messages
  return to the DLQ on every redrive while it does.
- **The data doesn't need the messages.** A transient failure writes no row (`retry candidate (no
row written)`), and `backfill-enrichment-runner`'s default drain takes every asset and NFT with
  no `*_enrichment` row (`main.rs`, `NOT IN (SELECT … FROM *_enrichment)`). Every key below is
  such a row, so the runner fetches it again without the DLQ. The drain only re-arms the alarm.

## The keys

### `testnet-enrichment-dlq` (46 messages, 33 distinct)

`sep1_assets` (`contract_id` 0 in every one):

| Sent        | Issuer id             | Asset code   | Asset type | Copies |
| ----------- | --------------------- | ------------ | ---------- | ------ |
| 11-05 14:21 | `6430918769488600577` | `M48EC4`     | 1          | 1      |
| 11-05 14:21 | `6430918769488600577` | `USDC`       | 1          | 4      |
| 11-05 14:21 | `8521636456879727766` | `T74A75`     | 1          | 1      |
| 11-05 14:21 | `8521636456879727766` | `USDC`       | 1          | 6      |
| 11-05 14:21 | `9062051846578321300` | `PC1A2A4B01` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC1ABE4B00` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC446C5900` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC489F2301` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC48A98500` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC5E1A7000` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC69C0BA00` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC6A515102` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC7B8F3700` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PC82EC5000` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PCAAE64601` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PCC86CCA02` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PCD06EC400` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PCE0E51C02` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PCE5ABE501` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PCEF86D300` | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PDDG0OHA`   | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PDDGDM61`   | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PDDGEI11`   | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PDDKS4LY`   | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `PDDKTBNU`   | 1          | 1      |
| 11-05 14:21 | `9062051846578321300` | `RDM1438`    | 1          | 6      |

`nft_token_uri`:

| Sent        | Contract id            | Token id | Copies |
| ----------- | ---------------------- | -------- | ------ |
| 11-05 14:21 | `-5731650001251159982` | `0`      | 1      |
| 11-05 14:21 | `832230481242508817`   | `13`     | 1      |
| 11-05 14:21 | `832230481242508817`   | `14`     | 1      |
| 11-05 14:21 | `832230481242508817`   | `15`     | 1      |
| 11-17 09:10 | `3646591691903889955`  | `2`      | 1      |
| 11-17 09:29 | `5980578433844636912`  | `1`      | 1      |
| 11-17 09:29 | `5980578433844636912`  | `2`      | 1      |

### `production-enrichment-dlq` (2 messages, 2 distinct)

`nft_token_uri`:

| Sent        | Contract id            | Token id | Copies |
| ----------- | ---------------------- | -------- | ------ |
| 11-16 22:16 | `-5731955624881490446` | `11`     | 1      |
| 11-17 20:36 | `-5731955624881490446` | `25`     | 1      |

## Production, read again on 2026-11-24

Both alarms were still in ALARM: production since 11-05 09:50, testnet since 11-19 13:39.
Production's DLQ held 10 messages: the 2 NFT messages above and 8 that arrived after 11-19.
None of the 8 paged, because the alarm was already in ALARM.

All 10 were received with `VisibilityTimeout=30`, so none was deleted. Each was hidden for 30 s.
The new 8 read `ApproximateReceiveCount` 4: three worker tries and this read. The 2 NFT messages
read 14 and 24, because the polls on 11-19 and today raised them.

`Sent` is the message's first send to the source queue, which the DLQ keeps. The failures are
in the production worker's log a few minutes later.

| Sent (UTC)        | Issuer id              | Asset code | Copies | Worker log (UTC)                                                  |
| ----------------- | ---------------------- | ---------- | ------ | ----------------------------------------------------------------- |
| 11-19 19:04       | `6271003847480919647`  | `XLM`      | 2      | `timeout fetching stellar.toml from wtg-qfs.com`, 19:05–19:08     |
| 11-19 19:04–19:05 | `6271003847480919647`  | `XRP`      | 2      | the same                                                          |
| 11-22 20:47       | `-8089211510502436884` | `WWG1WGA`  | 2      | `timeout fetching stellar.toml from wwg1wga-qfs.com`, 20:48–20:52 |
| 11-22 20:48       | `-8089211510502436884` | `USAT`     | 2      | the same                                                          |

All 8 are `sep1_assets` with `asset_type` 1 and `contract_id` 0. The first of them ages out
around 12-03 19:04 UTC.

### What it shows

- **One outage on a host can end in both the DLQ and a sentinel.** On 11-22 the same issuer's
  `BTC`, `WTI`, `GOLD` and `APXLM` first timed out too, then got a sentinel at 20:54–20:56:
  `sep1_fetch_permanent`, `error sending request for url
(https://wwg1wga-qfs.com/.well-known/stellar.toml)`. A timeout is `Sep1Error::Timeout` and
  retryable, so a message whose last try timed out ends in the DLQ. A connect-level failure is
  permanent (`http_transient.rs`), so it writes the `''` sentinel. The error the last try hit
  decides which.
- **The two hosts did not fail the same way.** Checked from a laptop on 11-24:
  `wwg1wga-qfs.com` answers 200 with a valid `stellar.toml`, so 11-22 was an outage, not a dead
  domain. `wtg-qfs.com` gets `SERVFAIL` from DNS. These 8 messages therefore don't show that
  timeouts are a dead-host signature, the condition under which `http_transient.rs` says to
  "move them across". One host came back.
- **The sentinel side is the one that needs repair.** `WWG1WGA` and `USAT` ended on a timeout,
  which writes no row, so the runner's default drain fetches them again, now that the host
  answers. `BTC`, `WTI`, `GOLD` and `APXLM` carry a sentinel, which only a `--retry-sentinels`
  run fetches again.
- **The data still doesn't need the messages.** A timeout writes no row (`retry candidate (no
row written)`: 6 lines each for `XLM` on 11-19 and `USAT` on 11-22, nothing after either).
