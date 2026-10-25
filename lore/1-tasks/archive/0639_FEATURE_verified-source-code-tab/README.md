---
id: '0639'
title: 'FEATURE: Verified Code tab — show source code verified via the BlockSmith Query API (testnet first)'
type: FEATURE
status: completed
related_adr: ['0052']
related_tasks: ['0465', '0553']
tags: [priority-medium, effort-medium, layer-api, layer-frontend, testnet]
links:
  - https://staging.sv.blocksmith.co/api/query/v1/docs
  - https://staging.sv.blocksmith.co/api/query/v1/openapi.json
  - https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0058.md
  - web/src/pages/contracts/ContractCode.tsx
  - crates/api/src/contracts/queries/get_decompiled.rs
history:
  - date: 2026-10-14
    status: backlog
    who: claude
    note: >
      Task created on a team request (2026-10-14). Query API measured on its
      staging host: 33 verified Wasm hashes, all deployed on our testnet
      database; findings in notes/R-blocksmith-query-api-measured.md.
  - date: 2026-10-15
    status: active
    who: karolkow
    note: >
      Activated after the design round: endpoints, tab name and states,
      layout (prototype variant D), caching, failure and download behaviour
      settled; three UX audits applied to the prototype. Open: the badge
      outside the tab and the `disputed` state, asked in the team.
  - date: 2026-10-16
    status: completed
    who: karolkow
    note: >
      Shipped on testnet 2026-10-16: #661 (endpoints), #662 (tab), #667
      (values shown as published), 45ae70a3a (unpack as tar and unzip do),
      #668 (plain chip). Served files equal tar/unzip output for all 7
      contracts with public source. The tab rename reaches mainnet with
      its next release.
---

# FEATURE: Verified Code tab — source verified via the BlockSmith Query API

## Summary

BlockSmith runs a Soroban source verification service, similar to Sourcify
on EVM: a contract author submits the source, a verifier rebuilds the Wasm
in a pinned container and compares the hash with the code on chain. Its
**Query API** aggregates the results of several verifiers. The contract page
gets a **Verified Code** tab showing that source, and the current **Code** tab
(soroban-ret decompilation, task 0465) is renamed **Decompilation**. The two
never mix: decompilation is always available and imperfect; verified source
is exact and present only when a verifier confirmed it.

## Stan teraz

Live on testnet since 2026-10-16; mainnet off until a production host.

## Context

Measured facts with commands (rate limits, coverage, archive shapes):
[notes/R-blocksmith-query-api-measured.md](notes/R-blocksmith-query-api-measured.md).
The Query API also serves mainnet (`/contracts/mainnet/{id}`): 12 of its 39
verified hashes run in 38 mainnet contracts (measured 2026-10-16).

## Decisions so far

Backend:

- 2026-10-14 — testnet only while the service runs on its staging host; the
  Query API URL is configuration, set for testnet only (ADR 0052).
- 2026-10-14 — two contract sub-resources in the `/decompiled` pattern:
  `GET /v1/contracts/{id}/verification` (small status, fetched with the
  page) and `GET /v1/contracts/{id}/verified-source` (unpacked files, fetched
  when the tab opens), behind a `runtime_enrichment` client like
  `wasm_code.rs`, no persistence. A ClickHouse sync of the `/wasms` feed is a
  later step, once the service leaves staging.
- 2026-10-15 — cache headers from the existing tiers: status `MEDIUM` (60 s,
  like contract detail), files `LONG` (300 s, like `/decompiled`).
- 2026-10-15 — the archive's sha256 is checked against `source_sha256`;
  a mismatch is an error. Unpacking has size and file-count caps.
- 2026-10-16 — the archive is unpacked as `tar` and `unzip` leave it:
  regular files only; folders, links and metadata entries skipped; a path
  listed twice kept once. Refusing such entries hid 4 of 6 real archives.
- 2026-10-15 — when the Query API fails: no tab, an error logged with its
  cause; a deep link to the tab shows `QueryErrorState` with Retry.
- 2026-10-16 — status from the verifiers' answers (`agreement` unread);
  any verifier's archive copy will do; a bad archive URL is a 500 (no
  chip), a failed download shows only on the tab, with Retry.
- 2026-10-15 — no verifier service on the network: `/verification` answers
  200 `unavailable` under `LONG`, asking neither database nor service.

Frontend (built from the Decompilation tab's parts):

- 2026-10-15 — the tab and the header chip show only for `verified` and
  `verified_private`; `mismatched`, `disputed` and everything else show
  nothing, as if no verification existed, and the backend serves no files
  for them. A deep link shows a loading state, never a fallback to
  Interface.
- 2026-10-15 — the tab is named **Verified Code**, with an emerald dot (the
  counterpart of Decompilation's amber "experimental" dot).
- 2026-10-15 — a **header chip** "Verified source" (design-system `Chip`,
  size md, like "Self-upgradeable") for both verified states; a plain
  label with a tooltip naming the verifiers, no click action (2026-10-16).
- 2026-10-14 — verified without public source: "Verified by <verifier
  names> — the source code is not published".
- 2026-10-14 — attribution line in the Decompilation footer's style:
  "Source verification by <verifiers from `verifier.name`> · aggregated by
  BlockSmith"; hidden when no verifier answered.
- 2026-10-14 — layout: file tree (WAI-ARIA tree, roving tabindex, filter
  via `DebouncedField`) beside `CodeHighlight` + `CopyButton`; one height
  per archive, the taller of tree and longest file, capped at 640 px; the
  open file in the URL (`?file=`); a stats line; a collapsed "Build recipe"
  box, no copy buttons, every value verbatim in a frame (2026-10-16).
- 2026-10-15 — Download links straight to the content-addressed archive on
  the verifier's storage.
- 2026-10-15 — below `md` the tree folds under "Files (N)"; recipe
  row "Verified at", stats "matches the published sha256", toolbar chip kept.
  "Code" → "Decompilation" on every network, a release note.
- 2026-10-16 — third-party values shown as published: recipe verbatim,
  sha256 as given, invisible and direction-changing characters written out
  as `⟨U+…⟩` with a warning (Copy keeps the original), no-break spaces kept.

## Not yet decided

- From BlockSmith: production host and timeline, trusted verifiers in
  production, archive formats as a closed list, `logo_url`.

## Out of scope

- The Verification (submission) API — authors use it, we do not.
- Mainnet, until the service has a production host.
- Indexing verification results into ClickHouse for now (later step above).

## Acceptance Criteria

- [x] A testnet contract with public verified source shows the Verified Code
      tab with its file tree and contents (e.g. `CD37GB5N…6E5EW`).
- [x] A verified contract without public source shows "the source code is
      not published" (e.g. `CCZOTIJC…M2XMH`).
- [x] `no_data`, 404 and an unreachable service show no tab, and the
      failure is logged with its cause, never shown as "not verified".
- [x] The archive's sha256 is checked against `source_sha256` before it is
      shown; a mismatch is an error, not a fallback.
- [x] The Code tab is renamed Decompilation; its behaviour is unchanged
      (testnet live; mainnet with its next release).
- [x] Mainnet has no Query API URL and never calls the service (ADR 0052).
- [x] **Docs updated** — `docs/architecture/**` for the new endpoints and
      the outbound call, per [ADR 0032](../../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [x] **API types regenerated** — the new endpoints touch `crates/api/**`.
