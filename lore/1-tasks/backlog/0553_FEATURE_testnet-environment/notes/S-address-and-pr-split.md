# Address and PR split (task 0553)

### Address — decided 2026-09-29

Testnet lives on subdomains of the same domain (`testnet.sorobanscan…` + an
API hostname), as its own deployment. Rejected: a path (`/testnet`) or a query
parameter (`?cluster=testnet`) on the mainnet site — both share one SPA build
across networks, so testnet could no longer run a newer frontend than mainnet
(the staging role), and both need router/cache changes in the SPA. Matches how
Stellar's own software is built: one instance and one database per network.

### PR split — decided 2026-09-29

| PR  | Scope                                                                                                                                                               | Production acts after merge                                               |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| A   | Ledger source, key prefix, bucket region and ClickHouse database from env; refuse a ledger folder of another network (API, indexer, backfill-runner)                | nothing — defaults are today's values                                     |
| B   | Infra refactor: `envName` widened, Makefile takes `ENV=`, RPC URLs and Galexie `START` into `envs/*.json`                                                           | nothing — `cdk diff` on production is empty                               |
| C   | ClickHouse tenant: `testnet_*` users, profile, quota and concurrency caps, Caddy CN map                                                                             | ClickHouse recreate (maintenance window)                                  |
| D   | Testnet environment on B: `testnet.json`, data-lake source, scheduled doorbell, stall alarm on its own channel, reset runbook, ADR 0052 accepted, architecture docs | deploy testnet, backfill from genesis — after the lake measurement passes |
| E   | TESTNET marker in the SPA shell (network switcher later)                                                                                                            | before the public start                                                   |

C is its own PR because production must act between it and D; B because a
mechanical move next to new infra would bury it, and it is proven by one
command. Stack depth two at most (B → D).

## Certs, API host, alarms

Decided 2026-09-30: Lambdas use their own certs mapped to the `testnet_*`
users — one project, but a misconfigured testnet cannot touch mainnet data,
and cert names already carry the environment. API host
`api-testnet-sorobanscan.rumblefishdev.com` behind Cloudflare with the same
Turnstile widget as mainnet and its own edge secret (each environment
generates one; corrected 2026-10-02); testnet alarms in their own Slack
channel.
