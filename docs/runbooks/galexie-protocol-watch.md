# Galexie protocol watch — what to do when it pages

`production-galexie-protocol-watch` says one of three things. Every pubnet
protocol vote stops a Galexie whose captive core does not support the new
protocol — protocol 29 stopped ours for ~14 h on 2026-10-01 — and the watch
exists so the bump is a planned task before the vote (task 0610).

## Read the reason

The alarm only says "the watch failed". The function's log says why:

```bash
aws logs tail /aws/lambda/production-galexie-protocol-watch --region eu-central-1 --since 2h
```

Every run logs one line starting with `OK:`, `LAGGING:`, `BEHIND:` or an
error that starts with `cannot determine` / `cannot read`.

## LAGGING — bump before the vote

The network's core already supports a protocol ours cannot apply; pubnet has
not voted yet. This is the lead time (P29: nine days between Horizon showing
core 29 and the vote).

1. The log's third line says whether Docker Hub has an image with the new
   core. "The bump is possible today" names the tag; a commit tag
   (`c927ffc`) is SDF's build ahead of the release tag and is what P29 would
   have shipped on.
2. Bump with the Galexie recipe in
   [`docs/deployment.md`](../deployment.md#galexie-live-ingestion--ingestion-stack)
   — mirror to ECR, pin the **ECR** digest, deploy the Ingestion stack.
3. The next run (within 30 min) logs `OK:`, and the alarm clears after it.

No image yet: nothing to do but wait; the watch keeps failing until one
appears, so the alarm stays in ALARM without paging again.

## BEHIND — the vote has happened

Pubnet already runs a protocol our core cannot apply; Galexie has stopped at
the vote ledger and `galexie-ingestion-lag` has paged too. Same bump as
LAGGING, now as an incident — then check that no ledgers are missing (the
continuity query in [`health.md`](health.md)).

## cannot determine / cannot read — the watch could not check

Nothing was compared. Read the error:

- `cannot read https://horizon.stellar.org/…` — Horizon was down for two
  runs in a row. If the next run is OK, nothing to do.
- `… is not in ECR` / `no Galexie container` / `no service` — the running
  task definition no longer matches what the watch expects (a renamed
  container or service, an image outside the `production-galexie` repo).
  Fix the watch in `infra/src/lib/stacks/galexie-protocol-watch.ts`.
- `carries no STELLAR_CORE_VERSION` — the image is not an SDF Galexie build;
  the watch cannot know its core. Confirm the version by hand
  (`stellar-core version` in the image) and fix the image or the watch.

A watch that cannot check is never treated as green: it is the same silence
that hid the P29 stall.
