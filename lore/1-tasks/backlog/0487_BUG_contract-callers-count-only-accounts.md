---
id: '0487'
title: 'BUG: contract callers count only accounts — "Unique callers 0" on 97% of contract pages, "—" on 27% of invocation rows'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0300', '0331', '0345', '0420']
tags:
  [backend, api, frontend, clickhouse, contracts, priority-high, effort-small]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Found on production while verifying the 0472 deploy: a contract with
      4,593,403 invocations in the window reported 0 unique callers. Root
      cause and blast radius measured against production ClickHouse before
      filing; every number below is measured, not estimated.
---

# BUG: a caller that is a contract is not a caller

## What is on screen

`/contracts/CB23WRDQWGSP6YPMY4UV5C4OW5CBTXKYN3XEATG7KJEZCXMJBYEHOUOV`:

```
Invocations (last 7 days)     4,593,403
Unique callers (last 7 days)  0
```

Four and a half million invocations by nobody. On the same page's
`Invocations` tab, every row's `Caller` cell is `—`.

## Root cause

`soroban_invocations_appearances` records the caller in **one of two**
columns — exactly one is non-null per row:

```
caller_id           Nullable(Int64)   -- the caller was an account
caller_contract_id  Nullable(Int64)   -- the caller was a contract
```

Every read path uses only the first:

| site                                          | what it does                            |
| --------------------------------------------- | --------------------------------------- |
| `crates/api/src/contracts/queries.rs:612`     | `uniqExact(sia.caller_id)`              |
| `crates/api/src/contracts/queries.rs:925`     | resolves `caller_id` → `caller_account` |
| `crates/api/src/transactions/queries.rs:1004` | same, on the transaction page           |

A contract invoked by other contracts therefore reports zero callers, and its
invocation rows render an em dash. That is the normal shape for a SAC used by
AMMs and farms — which is to say, for almost everything.

Same family as 0300 (`recent_events` hardcoded to zero in the same KPI strip):
a tile that states a confident number nobody produced.

## Measured on production

Read-only, against production ClickHouse, window = the same 7 days the API uses.

**The example contract (KALE SAC):**

|                                  | value     |
| -------------------------------- | --------- |
| rows in window                   | 4,593,403 |
| rows with a non-null `caller_id` | 0         |
| distinct `caller_contract_id`    | 8         |
| what the page shows              | 0         |
| what it should show              | 8         |

**Network-wide, contracts with any invocation in the window:**

| bucket                                   | contracts  |
| ---------------------------------------- | ---------- |
| shows `0` but has contract callers       | **16,434** |
| has account callers, shows a real number | 507        |
| genuinely zero callers                   | 0          |
| total                                    | 16,941     |

97% of contract detail pages state a wrong zero. No contract is genuinely
callerless, so the displayed zero is never correct.

**Row level, for the `Caller` column:**

| bucket                        | rows       | share   |
| ----------------------------- | ---------- | ------- |
| account caller (renders)      | 16,924,273 | 73%     |
| contract caller (renders `—`) | 6,257,194  | **27%** |
| total in window               | 23,181,467 |         |

The dash is not a formatting nit: it asserts "no caller" where the truth is
"a contract called this", which is the misleading-fallback shape — a plausible
wrong value is worse than an admitted gap.

## Fix

1. **The count.** Count the pair, not one column. Verified on production:
   `uniqExact(tuple(caller_id, caller_contract_id))` returns 8 for the example
   contract where the shipped `uniqExact(caller_id)` returns 0. Counting
   `coalesce(caller_id, caller_contract_id)` is NOT equivalent — account and
   contract surrogates are separate id spaces and can collide.
2. **The column.** The invocation DTOs carry only `caller_account`. Add the
   contract caller so the cell can render and link it; the frontend already
   has the chip and link vocabulary for a contract id. Nullable end to end —
   a row with neither caller stays a dash, and per the measurement above that
   case does not currently occur.
3. **Both call sites** — the contract page's tab and the transaction page's
   invocation list — read from the same shape; fix them together or the same
   dash survives on the other page.
4. Regenerate `libs/api-types` (CI gate `API types freshness`).

## Watch out

- The tile label says "Unique callers". Once contracts count, decide whether
  the number is "callers" (one number) or splits account vs contract callers.
  One number matches the label already on screen; a split invents a second
  tile nobody asked for.
- `sia FINAL` is already in these queries (0420) — the dedup is not the issue
  here and must not be removed while touching the SQL.
- `uniqExact` over a tuple is heavier than over one column. The existing query
  scans the same rows either way; measure before and after on the busiest
  contract rather than assuming the cost is free.

## Acceptance criteria

- [ ] Unique callers counts contract callers; the example contract reports 8
- [ ] `Caller` renders and links a contract caller instead of `—`
- [ ] Contract detail and transaction detail both fixed
- [ ] A test pins the account-caller, contract-caller and mixed cases
- [ ] **Docs updated** — `docs/architecture/**` backend/API surface for the
      contract endpoints, per ADR 0032
- [ ] **API types regenerated** — `npx nx run @rumblefish/api-types:generate`
      committed with the DTO change

## Notes

Found while verifying the 2026-08-17 production release; not reported from
outside, so no issue link. The audit that surfaced it also found API fields
the frontend never renders (`has_soroban` on four list endpoints, `fee_bps`
and two ledger columns on pool list items, `transaction_id` on events) — those
are wire cost without a reader, unrelated to this defect and not in scope here.
