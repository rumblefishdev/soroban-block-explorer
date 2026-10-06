# Galexie protocol watch — what to do when it pages

`production-galexie-protocol-watch` pages when a Galexie with a newer captive
core is on Docker Hub and ours is not on it, or when it could not check.
Every pubnet protocol vote stops a Galexie whose core does not support the
new protocol — protocol 29 stopped ours for ~14 h on 2026-10-01, while the
image that fixed it had been on Docker Hub for a week (task 0610).

## Read the reason

The alarm only says "the watch failed". The function's log says why:

```bash
aws logs tail /aws/lambda/production-galexie-protocol-watch --region eu-central-1 --since 13h
```

To run it now instead of waiting up to 6 h (after a deploy, say), invoke
the function — this runs the check, it changes nothing:

```bash
aws lambda invoke --region eu-central-1 --function-name production-galexie-protocol-watch /dev/stdout
```

Every run logs one line starting with `OK:` or `NEW CORE:`, or an error that
starts with `cannot determine` / `cannot read`.

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
4. Invoke the function (above): it logs `OK:`, and the alarm clears.

If the vote has already happened, Galexie has stopped at the vote ledger and
`galexie-ingestion-lag` has paged too. Same deploy, now as an incident — then
check that no ledgers are missing (the continuity query in
[`health.md`](health.md)).

## cannot determine / cannot read — the watch could not check

Nothing was compared, two runs in a row. Read the error:

- `cannot read https://hub.docker.com/…` or `…registry-1.docker.io…` —
  Docker Hub was down or rate-limited. If the next run is OK, nothing to do.
- `… is not in ECR` / `no Galexie container` / `no service` — the running
  task definition no longer matches what the watch expects (a renamed
  container or service, an image outside the `production-galexie` repo).
  Fix the watch in `infra/src/lib/stacks/galexie-protocol-watch.ts`.
- `carries no STELLAR_CORE_VERSION` — the image is not an SDF Galexie build;
  the watch cannot know its core. Confirm the version by hand
  (`stellar-core version` in the image) and fix the image or the watch.

A watch that cannot check is never treated as green: it is the same silence
that hid the P29 stall.
