---
id: '0513'
title: 'OPS: Caddy response_header_timeout cuts long ClickHouse queries — and the mount that would have swallowed the fix'
type: OPS
status: active
related_adr: []
related_tasks: ['0331', '0455', '0511', '0314']
tags:
  [
    'infra-hetzner',
    'caddy',
    'clickhouse',
    'deploy',
    'silent-failure',
    'cross-team',
    'priority-high',
    'effort-small',
  ]
links: []
history:
  - date: '2026-08-21'
    status: active
    who: stkrolikiewicz
    note: >
      Raised by the prices team (their 0111): their enrichment worker has
      failed every run since 2026-07-26, 18/18 measured at exactly 30.0 s.
      Verified on the box — the limit is real and live in the running
      process. Found a second defect while verifying: the Caddyfile bind
      mount is inode-pinned, so the fix would not have reached Caddy.
---

# OPS: Caddy response_header_timeout cuts long ClickHouse queries

## Summary

`infra-hetzner/Caddyfile` states a policy — timeouts "cover the longest
legitimate analytical query", a 7200 s window — and then contradicts it with
one knob: `response_header_timeout 30s`. Any query that cannot put a first
byte on the wire within 30 s is disconnected by the proxy while ClickHouse
runs it to completion, writes its rows and logs `QueryFinish`. The work
happens; the caller is told it failed.

This has cost another team 26 days of a broken pipeline and cost us one
operator intervention in 0331. Raising the knob to 7200 s is a one-line
change. Making it actually reach the running proxy is not — see Context B.

## Context A — the limit, verified on the box 2026-08-21

| what                             | measured                                                 |
| -------------------------------- | -------------------------------------------------------- |
| `Caddyfile` on the box           | `response_header_timeout 30s` (line 242)                 |
| Caddy admin API, in-memory       | `"response_header_timeout":30000000000` — 30 s, live     |
| `read_timeout` / `write_timeout` | `7200000000000` — the policy the same block declares     |
| `app-caddy-1`                    | `StartedAt 2026-06-29T07:36:53Z`, `RestartCount 0`       |
| last deploy                      | `DEPLOYED_INFO` — sha `8eafc3b6`, `2026-07-06T14:01:36Z` |

The value is not just declared in a file; it is what the process enforces.

### It is not a property of `INSERT … SELECT`

The knob bounds **time to first byte**, nothing else. A streaming `SELECT`
emits headers in milliseconds and may then run for the full two hours. A
query that buffers — `INSERT … SELECT`, `CREATE TABLE AS`, a heavy
`GROUP BY`, `OPTIMIZE` — puts nothing on the wire until it finishes, so it
dies at 30 s regardless of shape. Ours died this way in 0331: a ~6 min,
~9.5 bn row scan, a plain `SELECT`.

The consequence matters for the "we are removing a safeguard" objection:
**30 s never bounded a runaway.** A runaway streams, so it passes. What the
knob selected for was the class of query doing real work.

### Nothing changed on our side on 2026-07-26

Caddy has not restarted since 2026-06-29 and the config has not changed
since 2026-07-02. The reporting team's own explanation — their query crossed
30 s as their table grew and never came back under — is the only one the
evidence supports. Recorded because the first hypothesis on our side (a
deploy reverted an operator's fix) was wrong and should not be repeated.

### Our exposure is worse than a red log line

`crates/db-clickhouse/src/persist.rs:303` propagates the error and aborts the
ledger/partition so it retries. If ClickHouse committed while Caddy
disconnected us, the retry writes a second copy. Most tables are RMT and
absorb that; the 12 Tier-1 columns needing MIN semantics do not, which is why
`repair-tier1` exists (`docs/backfills.md`). The candidate for hitting this
first is `query_sac_classic_map` — a `GROUP BY` over all of `asset_sac`,
buffering, growing with the table. Exactly the shape that crossed the
threshold for the other team.

## Context B — the mount would have swallowed the fix

`/srv/app/infra-hetzner/Caddyfile` is bind-mounted as a **single file**, which
pins the inode. Ansible syncs the subtree with rsync, which writes temp + rename
— a new inode every time.

| path                                    | inode      | mtime               |
| --------------------------------------- | ---------- | ------------------- |
| host `/srv/app/infra-hetzner/Caddyfile` | `16777224` | 2026-06-09 08:10    |
| container `/etc/caddy/Caddyfile`        | `16777223` | 2026-07-02 13:59:41 |

Two different files. Since the 2026-07-06 deploy the container has been
reading an orphan — the file the operator edited and reverted during 0331.
Nothing broke only because both copies happen to say 30 s.

Had we shipped the one-liner the normal way, the result would have been a
phantom deploy: rsync rewrites inode `…224`, the checksum sentinel correctly
reports "changed" and fires `Reload caddy`, `caddy reload` re-reads
`/etc/caddy/Caddyfile` — still the orphan, still 30 s. Repo green, box green,
handler green, nothing changed.

Same class as the 2026-07-06 `prices_writer` grant incident (0314), where a
`users.d` XML change needed `--force-recreate clickhouse`; the box still shows
it — clickhouse `StartedAt 2026-07-06T14:58`, caddy `2026-06-29`. The snippet
mount already avoids this by mounting a directory; the Caddyfile does not.

## Implementation

### Step 1 — raise the knob

`infra-hetzner/Caddyfile`: `response_header_timeout 30s` → `7200s`. The
comment above it justifies the tighter value as "tighter than the CH-side
timeout so Caddy releases the upstream socket first on stalls" — true of a
stalled upstream, false of a working query. Correct it rather than delete it.

### Step 2 — stop the inode from escaping

`infra-hetzner/ansible/roles/app/tasks/main.yml`: add `--inplace` to
`rsync_opts` on **both** sync tasks. rsync then writes into the existing file
instead of replacing it, the inode survives every deploy, and the existing
zero-downtime `caddy reload` handler starts meaning something again. The trade
— a torn write is no longer impossible — is acceptable because Caddy validates
on reload and keeps the old config on a parse error.

The second task (`Sync ClickHouse config + schema`) has the identical defect
and a prior victim: ten `config.d/*.xml` + `users.d/*.xml` files are
bind-mounted into `app-clickhouse-1` individually, and the 2026-07-06
`prices_writer` grant change synced cleanly, reported success and did nothing
until the container was force-recreated (0314). Same root cause, found only
because this task named it.

`--inplace` does not rescue the current container: it is already pinned to the
orphan. One `--force-recreate caddy` at this deploy re-anchors it.

### Step 3 — write down both, next to the sibling that is already documented

`infra-hetzner/README.md`: a note beside the existing `--force-recreate
clickhouse` log-rotation gotcha, and a line in "Post-deploy verification" that
reads the timeout back **from the admin API**, not from the file.

## Acceptance Criteria

- [ ] Caddy's admin API reports `"response_header_timeout":7200000000000` on
      the production box
- [ ] Host and container inode for the Caddyfile are identical after the deploy
- [ ] A subsequent `--tags app` run leaves the inode unchanged (`--inplace`
      verified, not assumed)
- [ ] The reporting team confirms their worker's 30 s failures stopped
- [ ] **Docs updated** — `infra-hetzner/README.md` carries the mount gotcha and
      the admin-API verification. `docs/architecture/**` N/A: a timeout value
      and a deploy mechanic are not the shape of the system.
- [ ] **API types regenerated** — N/A, nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**`.

## Notes

An instance for [0455](0455_OPS_observability-umbrella-declared-vs-actual-and-silent-failure/README.md)'s
defect 1, with a twist worth recording: here "declared vs actual" is not repo
versus box — both agreed — but **the file on the box versus the memory of the
process reading it**. The probe already exists and nothing calls it; it is the
container's own healthcheck endpoint:

```bash
ssh sorban-prod 'docker exec app-caddy-1 wget -qO- http://127.0.0.1:2019/config/' \
  | tr '{,}' '\n' | grep -i timeout
```

Adjacent but not overlapping: [0511](0511_REFACTOR_infra-config-is-not-one-thing.md)
covers three other places where one declaration has two homes.

The other team is setting `max_execution_time` per caller on their side. We are
not adding one to the write profiles (`write_no_ddl`, `prices_write_ddl`,
`admin`) in this task: they have never had one, the 30 s knob never provided
it, and no query has actually run away. `api_reader` already caps at 30 s
CH-side (`crates/db-clickhouse/users.d/profiles.xml:27`), so the read path is
bounded where it should be. One line per profile if that ever changes.
