# Implementation phases from ADR 0052 (task 0553)

### Implementation

Phases from the ADR, in dependency order — 1 first, 2 before anything connects.

1. **Config.** `infra/envs/testnet.json` + `infra/src/bin/testnet.ts` (mirror
   of `production.ts`); widen `envName`; parameterise the Makefile by
   environment; move into config the RPC URL list, the Galexie `START`
   ledgers (item 8) and the archive prefix — for BOTH readers, the API's
   `stellar_archive` and `backfill-runner` (items 4, 9), including the
   history-archive URL and the ledger floor.
   Galexie at 1 vCPU / 4 GB — Stellar's own testnet sizing, not the 13 GiB
   mainnet figure (captive-core memory scales with network state, not cadence).
2. **ClickHouse tenant.** `CREATE DATABASE testnet`; `apply_init_sql` with
   `CLICKHOUSE_DATABASE=testnet`; `testnet_writer` / `testnet_reader` in
   `users.d/services.xml` with grants `ON testnet.*`, profile and quota copies
   in `profiles.xml` / `quotas.xml` (the `prices_*` blocks are the template);
   CN pairs added to `CLICKHOUSE_CN_USER_MAP` (`group_vars/all.yml:104`,
   [[0240]]); client certs under `soroban/testnet/mtls/*`. A `users.d` change
   applies only on `docker compose up -d --force-recreate clickhouse` — the
   bind-mounted files are inode-pinned (0314). Quota: a bounded `read_rows`
   cap, remembering it is a hard error when tripped (0290), not a throttle.
   The testnet reader does read `prices.*` (item 11, decided 2026-09-30).
3. **Ingestion.** `Explorer-testnet-{Network,LedgerBucket,Ingestion}`.
   **Decided 2026-09-28: from the current testnet genesis**, backfilled from
   the public data lake (`…/ledgers/testnet/<genesis-date>/`), then live. Cheap:
   one 64k-ledger partition measured 113 MB at genesis and 1.03 GB at the tip,
   so the whole history is tens of GB (estimate from those two) against
   ~13 GB per mainnet partition. Each reset then re-runs the backfill path.
4. **Compute.** `Explorer-testnet-{Compute,ApiGateway,CloudWatch}`; alarms go
   to the same Slack topic and must say which environment fired.
   **Decided 2026-09-28: a reset alarm** (item 10). With the data lake it
   becomes the stall alarm — see "Doorbell cadence" (2026-09-30).
5. **SPA.** `Explorer-testnet-Delivery` at `testnet.sorobanscan.rumblefish.dev`,
   a TESTNET marker in the shell, the API hostname either via `rf-domains` or
   the legacy Route 53 path.
6. **CI.** `deploy-testnet.yml` from the dispatch/tag template of [[0390]];
   GitHub environment `testnet` with its own role from `cicd-stack.ts`.
   Auto-deploy from `develop` first; the `testnet` branch comes when a stable
   promotion point is wanted (ADR §3).
7. **Runbook.** Testnet reset: `DROP DATABASE testnet`, re-init, point the
   archive prefix at the new genesis folder, backfill, restart Galexie from the
   new genesis. Triggered by the reset alarm (phase 4). Lives in
   `docs/runbooks/`.

Cost, estimated 2026-09-01 from the production baseline: **~$80/month net, no
new hardware** — Galexie at 1 vCPU / 4 GB is ~$51 of it. Testnet closes ledgers
on the same ~5 s cadence, so S3 request and Lambda invocation counts barely
fall; only object sizes do. Do not re-estimate it from "testnet has few
transactions".

### Notes

- Testnet is **functional** staging, not performance staging (ADR caveat): a
  handful of accounts against mainnet's 22M reproduces none of the 0357-class
  scale problems. Mainnet byte-identical verification stays.
- Testnet runs protocol upgrades weeks ahead of pubnet — [[0548]] found it on
  protocol 28 on 2026-08-27, before the pubnet vote. A testnet explorer is
  therefore also the early warning for the XDR and Galexie breakages that
  0368 and 0548 handled after the fact.
- **Protocol lead, 2026-09-29.** Testnet upgraded 15–20 days before mainnet for
  P22–P28 (P24: one day). The protocol-27 freeze of the mainnet indexer (0368,
  2026-07-09) would have shown on testnet on 2026-06-18. Consequences: the
  stellar-xdr bump ships to testnet first and must still decode the older
  mainnet protocol; the testnet indexer stalls on each testnet upgrade until it
  does, so the stall alarm must say "testnet" and not page as a production
  incident. **Gap:** reading the data lake, testnet never exercises OUR Galexie,
  so a stale-core stall like 0367 is not caught. Cover it in the upgrade
  runbook: a one-off run of the new Galexie image against testnet before the
  mainnet vote.
