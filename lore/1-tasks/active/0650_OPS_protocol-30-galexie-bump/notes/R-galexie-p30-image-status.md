---
title: 'Galexie protocol-30 image: status on 2026-10-19'
type: research
status: mature
---

# Galexie protocol-30 image: status on 2026-10-19

**Image status** (measured: Docker Hub tags API, GitHub API on
`stellar/stellar-galexie`, 2026-10-19).

| Where                  | Newest                                                                                    | Captive core                                                                            |
| ---------------------- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Hub release tag        | `29.0.0` (2026-10-01), = `latest`                                                         | `29.0.0-3589.4eb833373`                                                                 |
| Hub sha tags           | `00a8c31`, `728de49` (2026-10-14, from `main`)                                            | 29 — `main`'s workflow pins `29.0.0-3589.4eb833373.noble`                               |
| GitHub releases        | `galexie-v29.0.0`                                                                         | —                                                                                       |
| Branch `protocol-next` | `47e46de` — PR #98 "Protocol 30: build against the P30 go-stellar-sdk", merged 2026-10-15 | workflow pins RC `30.0.0-3642.0e49751aa.noble`, comment "move to 30.0.0 before release" |

**When it appears on Hub** (source: `.github/workflows/galexie.yml` on `main`
and `protocol-next`). CI pushes a sha-tagged image only on pushes to `main`
and `release/**`, and a release tag on `galexie-v*` tags (`-rcN` included).
`protocol-next` is neither, so its build is not on Hub. The image appears when
`protocol-next` is merged to `main` or a `release/v30.*` branch is pushed.
`main` is 2 commits ahead and 2 behind `protocol-next` (compare API).

**Core alone is not enough** (derived: PR #98 body + 0649 notes). PR #98
bumps go-stellar-sdk for CAP-0084/87/88. CAP-0088 adds mandatory
`StellarValueExt` arms, and Galexie reads `LedgerCloseMeta` through that SDK.
So the image must contain commit `47e46de` (or its successor on `main`), not
only a core-30 package on top of `main`'s current code.

**Lead time** (source: 0610 note `R-image-lead-time-p27-p29.md`). The first
image with the new core came 4 weeks, ~4–6 weeks, and 7 days before the P27,
P28, P29 votes. For P29 the release tag came 3.5 h _after_ the vote; only the
sha tag `c927ffc` came before. So act on the sha tag of the bump commit, do not
wait for the release tag.

**Core 30 applies protocol-29 ledgers** (source: 0649 notes — mainnet and
testnet validators already run stellar-core v30.0.0 on protocol 29). So the
deploy can go before the vote.

**Testnet runs no Galexie** (source: `infra/envs/testnet.json → ledgerSource:
public-lake`). The testnet vote does not touch this task; its
`galexieImageTag` (28.0.1) is unused.

**ECR `production-galexie`** (measured: read-only `aws ecr describe-images`):

| Pushed (UTC+2) | Tag                        | Digest                                                                                           |
| -------------- | -------------------------- | ------------------------------------------------------------------------------------------------ |
| 2026-05-26     | `26.1.0`                   | `sha256:e60c8f8d…`                                                                               |
| 2026-07-09     | `galexie-27.0.0`, `latest` | `sha256:91eae7af…`                                                                               |
| 2026-09-14     | `28.0.1`                   | `sha256:1d511631…`                                                                               |
| 2026-10-01     | `29.0.0`                   | `sha256:5269dfd9…` (manifest list; its amd64 entry `sha256:538ad0fb…` = the Hub `29.0.0` digest) |

`infra/envs/production.json → galexieImageTag` = `sha256:5269dfd9…d495a3`.
