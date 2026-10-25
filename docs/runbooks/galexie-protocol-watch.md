# Galexie protocol watch — what to do when it pages

`production-galexie-protocol-watch` pages when a protocol upgrade is coming
and we are not ready for it, or when it could not check. It runs two checks:

- **Galexie** (task 0610): a Galexie with a newer captive core is on Docker
  Hub and ours is not on it. Every pubnet protocol vote stops a Galexie whose
  core does not support the new protocol — protocol 29 stopped ours for
  ~14 h on 2026-10-01, while the image that fixed it had been on Docker Hub
  for a week.
- **stellar-xdr** (task 0646): core supports a protocol, a
  `soroban-env-host` built on that protocol's `stellar-xdr` is published, and
  our indexer is not built with it. A protocol that
  changes the XDR stops the indexer even with a current Galexie (protocol 29
  did not change it; 30 does).

## Read the reason

The alarm only says "the watch failed". The function's log says why:

```bash
aws logs tail /aws/lambda/production-galexie-protocol-watch --region eu-central-1 --since 7h
```

To run it now instead of waiting up to 3 h (after a deploy, say), invoke
the function — this runs the check, it changes nothing:

```bash
aws lambda invoke --region eu-central-1 --function-name production-galexie-protocol-watch /dev/stdout
```

Every run logs two lines, one per check. The Galexie line starts with `OK:`
or `NEW CORE:`, the stellar-xdr line with `OK:`, `WAITING:` or `BUMP:`;
either can instead be an error starting with `cannot determine` /
`cannot read`. The alarm fires when either line is not `OK:` or `WAITING:`.

## Right after the first deploy

The alarm has no data until the first run, and no data counts as failed, so
it may page once and clear after the first 6-hour period with runs. That
message is the check that the alarm reaches Slack. Invoke the function once
after the deploy to see the first `OK:` in the log.

Deploy the Compute stack before the CloudWatch stack: the stellar-xdr check
reads `STELLAR_XDR_MAJOR` from the indexer's configuration, which Compute
sets, and without it the run fails with `cannot determine`.

## NEW CORE — deploy the new Galexie before the vote

The log names the Docker Hub tag and its core. SDF published that image
before the vote in every case measured: P27 four weeks ahead, P28 four to
six, P29 seven days. An older protocol keeps working on the newer core, so
deploying early is safe.

1. Prefer a release tag (`29.0.0`) when one has the same core. A commit tag
   (`c927ffc`) is SDF's own build; P29's commit tag was the very image later
   tagged `29.0.0`, but P28's first core-28 image (`e746a5b`) came before the
   release — check the [tag list](https://hub.docker.com/r/stellar/stellar-galexie/tags).
2. The vote date is in SDF's upgrade announcement (the `stellar-dev` mailing
   list, the protocol's upgrade guide on the Stellar blog). If the vote
   needs a specific patch release, it says so there; the watch compares only
   the major version.
3. Deploy with the Galexie recipe in
   [`docs/deployment.md`](../deployment.md#galexie-live-ingestion--ingestion-stack)
   — mirror to ECR, pin the **ECR** digest, deploy the Ingestion stack.
4. Invoke the function (above): it logs `OK:`. The alarm clears when its
   6-hour period ends, within 6 h.

If the vote has already happened, Galexie has stopped at the vote ledger and
`galexie-ingestion-lag` has paged too. Same deploy, now as an incident — then
check that no ledgers are missing (the continuity query in
[`health.md`](health.md)).

## BUMP — bump stellar-xdr

The indexer decodes ledgers with the `stellar-xdr` crate, whose major is the
protocol it reads. `crates/contract-executor` hands our XDR types to
`soroban-env-host`, which pins one `stellar-xdr` exactly, so the two move in
one PR (see the comment in `crates/contract-executor/Cargo.toml`). The watch
therefore looks for a **`soroban-env-host` release** on a newer `stellar-xdr`,
read from `soroban-env-common`, which ships with each `soroban-env-host` under
the same number (29.0.2 → `stellar-xdr =28.0.0`, 30.0.0 → `stellar-xdr
=30.0.0`). The line names the release to bump to.

- **WAITING** (green) — core supports a newer protocol, but no
  `soroban-env-host` on a fitting `stellar-xdr` is published, so there is
  nothing to bump to. Fitting for protocol N means a major above ours and not
  above N: 30 is not for 29. On 2026-10-16 `stellar-xdr` 30.0.0 was out and
  the newest `soroban-env-host` (29.0.2) still sat on 28 — WAITING. It turns
  into BUMP the day such a release is published.
- **BUMP** — such a release is published. Whether mainnet has voted yet is in
  Horizon's root document (`current_protocol_version`); after the vote the
  indexer may stop at the next ledger that uses it
  (`production-ingestion-stall` pages then).

Steps:

1. Bump `soroban-env-host` in `crates/contract-executor/Cargo.toml` to the
   named release and the workspace `stellar-xdr` to the major it is built on.
2. Check the crate's XDR commit, not only its number: it must be the XDR core
   uses for that protocol.
3. Tell the stellar-prices-api team when the bump merges. Their workspace
   takes our `xdr-parser` from `develop` by git and pins its own
   `stellar-xdr` exactly. Their `Cargo.lock` holds one commit of ours, so
   our merge does not break them; their next update of `xdr-parser` puts two
   `stellar-xdr` majors in their build — a compile error until they bump
   their pin in the same change.
4. Run the parser and the ClickHouse
   e2e tests, and deploy to **testnet first** — testnet votes 15–20 days
   before mainnet. Then mainnet's Compute stack, before the vote
   ([`docs/deployment.md`](../deployment.md)). The deploy sets the indexer's
   `STELLAR_XDR_MAJOR`; invoke the function and it logs `OK:`.

## cannot determine / cannot read — the watch could not check

Nothing was compared, in both runs of a 6-hour period. Read the error;
anything without one of these prefixes is an AWS error (access, throttling)
and reads as itself:

- `cannot read https://hub.docker.com/…` or `…registry-1.docker.io…` —
  Docker Hub was down or rate-limited. If the next run is OK, nothing to do.
- `… is not in ECR` / `no Galexie container` / `no service` — the running
  task definition no longer matches what the watch expects (a renamed
  container or service, an image outside the `production-galexie` repo).
  Fix the watch in `infra/src/lib/stacks/galexie-protocol-watch.ts`.
- `carries no STELLAR_CORE_VERSION` — the image is not an SDF Galexie build;
  the watch cannot know its core. Confirm the version by hand
  (`stellar-core version` in the image) and fix the image or the watch.
- `cannot read https://horizon.stellar.org/` / `…crates.io…` — Horizon or
  crates.io was down. If the next run is OK, nothing to do.
- `Horizon reports …` / `crates.io lists …` / `no soroban-env-host release
was read` — the document changed shape;
  fix `infra/lambdas/galexie-protocol-watch/xdr.mjs`.
- `… has STELLAR_XDR_MAJOR=undefined` — the indexer was deployed without it:
  deploy the Compute stack.

A watch that cannot check is never treated as green: it is the same silence
that hid the P29 stall.
