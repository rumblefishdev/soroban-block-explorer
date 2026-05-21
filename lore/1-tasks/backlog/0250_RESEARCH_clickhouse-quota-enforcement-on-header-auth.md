---
id: '0250'
title: 'RESEARCH: ClickHouse quota enforcement gap on `X-ClickHouse-User` header auth path'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0240']
tags:
  [
    priority-medium,
    effort-small,
    clickhouse,
    security,
    defense-in-depth,
    investigation,
  ]
milestone: 2
links: []
history:
  - date: '2026-05-21'
    status: backlog
    who: fmazur
    note: 'Spawned from [[task-0240]] Phase 4. Verified empirically that CH 26.3.10 does not increment quota counters for requests authenticated via the `X-ClickHouse-User` HTTP header (the path Caddy uses in our proxy-trust model). URL param `?user=` and TCP native auth DO increment. CH refuses to mix the two (`Invalid authentication`), so the obvious "set both" workaround is blocked. Quotas (api_throttle, low_volume, high_write) are effectively no-op for Caddy-proxied traffic. Accepted as a known limitation in 0240; this task scopes the investigation + decision to either fix or accept long-term.'
---

# RESEARCH: ClickHouse quota enforcement gap on `X-ClickHouse-User` header auth path

## Summary

CH 26.3.10 does not count requests authenticated via `X-ClickHouse-User`
HTTP header against the user's quota. Our production auth path (Caddy
proxy-trust → `X-ClickHouse-User: <user>` header) therefore bypasses
the per-user query / row / bytes / execution-time caps declared in
`users.d/quotas.xml`. Decide whether to fix (and how) or accept as
permanent — document the answer.

## Context

[[task-0240]] introduced per-service ClickHouse users + RBAC profiles

- quotas. Phase 4 quota smoke test (10 001 `SELECT 1` queries as
  `api_reader` via Caddy header path) showed all 10 001 returning
  HTTP 200 with the CH quota counter stuck at 0. Subsequent probes
  confirmed:

* URL param `?user=api_reader` → counter increments correctly.
* TCP native (`clickhouse-client --user api_reader`) → counter
  increments correctly.
* HTTP header `X-ClickHouse-User: api_reader` → counter stays at 0.

CH explicitly rejects mixing the two paths in the same request
(`Invalid authentication: it is not allowed to use X-ClickHouse
HTTP headers and authentication via parameters simultaneously`),
so a "send both" workaround is unreachable.

Quotas remain defined in `quotas.xml` (host-side path uses them,
they document intent, future fix is a no-config-change unlock), and
DoS protection on the Caddy-proxied path falls back to other layers
(AWS API Gateway throttle, Caddy `request_body { max_size 256MB }`,
profile `max_execution_time` + `max_memory_usage`, host firewall).

This is acceptable for the current cutover but creates a long-term
asymmetry between declared policy and enforced policy. The
investigation answers whether and how to close the gap.

## Questions to answer

1. **Is this a CH 26.3.10 bug, an older bug fixed upstream, or
   intentional?** Search the ClickHouse GitHub issue tracker for
   existing reports on `X-ClickHouse-User` + quota interaction.
   Check release notes for 26.4 / 26.5 / 27.x for quota-related
   fixes.
2. **If a fix exists upstream**, what version contains it? Is the
   upgrade path safe (changelog review for breaking changes
   relative to our 26.3 pin)?
3. **If no upstream fix exists**, can we file a clean repro and
   either upstream a fix or get a maintainer answer on
   intent-vs-bug?
4. **Alternative: Caddy URL-rewrite** — modify `infra-hetzner/Caddyfile`
   to rewrite the request as `?user={ch_user}` instead of (or as
   well as, after stripping the client's `?user=`) the
   `X-ClickHouse-User` header. Test that:
   - Caddy reliably strips any client-supplied `?user=` or
     `X-ClickHouse-User` before adding its own.
   - URL-rewrite path triggers quota counting.
   - No new auth-mixing rejection (`Invalid authentication`) appears.
   - mTLS cert subject is still forwarded for audit
     (`X-Client-Subject` header is independent of auth).
5. **Alternative: `max_concurrent_queries_for_user`** — add this
   setting to each profile as a CH-side rate guardrail that works
   regardless of auth path. Decide a reasonable cap per profile
   (`read_only` ~50? `write_no_ddl` ~10? `partition_only` ~2?).
   This does NOT replace quotas conceptually (it caps concurrent,
   not rate) but mitigates the same "many small queries DoS"
   scenario.
6. **Cost/benefit of each option** — given that AWS API Gateway
   already throttles Lambda API at 50 req/s sustained / 100 burst,
   how much marginal DoS protection does CH-side rate limiting buy?

## Out of scope

- Implementing whichever fix the investigation recommends. Spawn a
  FEATURE follow-up for that work (`infra-hetzner/Caddyfile` edit
  - smoke tests OR CH version bump in `docker-compose*.yml` + dev
  - prod re-test).
- Migrating away from XML-managed users to SQL-managed (relevant
  but orthogonal — covered separately in 0240's "Out of Scope").

## Acceptance Criteria

- [ ] Notes in `notes/R-*.md` summarising the upstream investigation
      (existing issues, fixed versions, release-note pointers).
- [ ] Notes in `notes/R-*.md` summarising empirical retest of the
      Caddy URL-rewrite path on a sandbox stack.
- [ ] Notes in `notes/R-*.md` summarising empirical retest of
      `max_concurrent_queries_for_user` profile setting on a
      sandbox stack.
- [ ] `notes/S-decision.md` recording the chosen path:
      A) accept permanently (rate limit lives elsewhere, document
      in `clickhouse-rbac.md`),
      B) upgrade CH version (file PR with version bump),
      C) Caddy URL-rewrite (spawn FEATURE follow-up),
      D) add `max_concurrent_queries_for_user` (spawn FEATURE
      follow-up),
      E) combination.
- [ ] If a FEATURE follow-up is spawned, link the new task ID in
      this task's frontmatter `related_tasks`.
- [ ] **API types regenerated** — N/A (no `crates/api/**` /
      `libs/api-types/**` changes from a research task).
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md`
      "Known limitations" section refreshed with the conclusion
      (either confirming the permanent acceptance with stronger
      justification, or pointing at the spawned FEATURE task that
      fixes it).

## Dependencies

- [[task-0240]] must be merged first (the security doc + the
  current quota config are the baseline this task reasons about).

## Risks / Considerations

- **Upstream churn.** Filing an issue or pulling a fix from a
  newer CH version adds external coordination. If 26.4+ contains
  the fix as a normal bug-fix, upgrade is cheap; if the answer is
  "this is intentional, file an RFC", time investment grows.
- **CH version upgrade compatibility.** Our pilot + production
  workload depends on 26.3-specific behaviour (timeouts.xml
  empirically tuned, FREEZE + ATTACH PART transport ADR 0045,
  prometheus.xml, memory.xml all targeted at 26.x). Any version
  bump triggered by this investigation must run the full
  Phase 0+1 verification battery from 0240 on the new version
  before landing.
- **Caddy `rewrite` semantics.** Caddy 2.x query-string rewrite
  needs careful testing — easy to accidentally widen the
  surface (e.g. not stripping client `?user=` lets a client
  smuggle an arbitrary user name, defeating the proxy-trust
  identity assertion).
- **Low overall urgency.** Existing DoS protections (API Gateway,
  Caddy body size, profile execution caps) are not zero —
  accepting the gap is defensible. The investigation is about
  closing a documented asymmetry between policy and enforcement,
  not stopping an active attack vector.
