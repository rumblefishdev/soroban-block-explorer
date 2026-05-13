# Pre-audit finding: accounts.home_domain not backfilled from pre-existing state

**Date:** 2026-05-13
**Status:** open
**Source:** Step 0 mini-spike of task 0197 (DB completeness audit), enrichment drain spot-check
**Severity:** high — silently caps SEP-1 enrichment coverage at ~1.4% on a fresh local backfill

## TL;DR

The indexer writes `accounts.home_domain` only when it observes a
SetOptions operation (or other op that mutates the field) within the
indexed ledger range. For long-running issuers (Circle USDC, lobstr
AQUA, large classic credits in general), the SetOptions that
established their `home_domain` happened years ago — far outside any
realistic backfill window. The account row gets created later as a
side-effect (e.g. issuer appears as payment counterparty), but with
`home_domain = NULL`. SEP-1 enrichment then sees an empty
`home_domain`, sentinel-skips, and writes `''` to `icon_url` and
`name`. The pipeline is technically correct (fail-soft, sentinel-aware)
but produces no real coverage on production-shape data.

## Repro

Local backfill of pubnet ledgers `50944000..50955110` on 2026-05-13
(11111 ledgers, ~10% of one 64k-ledger partition):

After workaround-seeding `assets` rows from `account_balances_current`
(see [classic-credit-asset-row-missing finding](./2026-05-13-pre-audit-finding-classic-credit-asset-row-missing.md))
and running `backfill-enrichment-runner sep1-assets`:

```text
Total assets:          10369
icon_url populated:    141  (1.4%)
icon_url = '':         9483 (91.5% — sentinel: TOML fetched / issuer no image / home_domain empty)
icon_url IS NULL:      745  (7.2% — transient HTTP error)
```

Spot-check of well-known classic credits:

```sql
SELECT a.asset_code, acc.account_id AS issuer, acc.home_domain, a.icon_url
FROM assets a JOIN accounts acc ON acc.id = a.issuer_id
WHERE a.asset_code IN ('USDC','AQUA','BTC','ETH','yXLM')
ORDER BY a.asset_code;
```

| asset_code | issuer (prefix)             | home_domain             | icon_url         |
| ---------- | --------------------------- | ----------------------- | ---------------- |
| AQUA       | GBNZILSTVQ…                 | _(empty)_               | `''`             |
| USDC       | GA5ZSEJYB3… (Circle)        | _(empty)_               | `''`             |
| BTC        | GA4ZONZXVJ… (lobstr.co)     | `lobstr.co`             | `''`             |
| BTC        | GDDKCF4PQM… (lobstr.com.co) | `lobstr.com.co`         | `''`             |
| ETH        | GCQHZQ2BIF… (gold-treasury) | `www.gold-treasury.com` | NULL (transient) |

Successful enrichments (out of 141 real `icon_url` values, all from
smaller / younger issuers whose SetOptions lands inside the backfill
window):

| asset_code | home_domain                    | icon_url                                               |
| ---------- | ------------------------------ | ------------------------------------------------------ |
| RIL        | `in.indus.exchange`            | `https://indus.exchange/images/ril.png`                |
| governICE  | `aqua.network`                 | `https://aqua.network/assets/img/ice-logo.png`         |
| GRPB       | `mexico.indus.exchange`        | `https://mexico.indus.exchange/images/grpb.png`        |
| Pfizer     | `in.indus.exchange`            | `https://indus.exchange/images/pfizer.png`             |
| PLZL       | `gold.stellar-commodities.com` | `https://gold.stellar-commodities.com/images/plzl.png` |

The pipeline works correctly when `home_domain` is populated. The
problem is upstream: it is not populated for most production issuers
in a short backfill window.

## Root cause

`accounts.home_domain` is populated from one of:

1. `LedgerEntryChange` of type `Created` for the account (when the
   account itself first appears on-chain inside the indexed window).
2. `SetOptions` op that explicitly sets `home_domain` (observed inside
   the indexed window).

For pubnet issuers that have existed for years (Circle, lobstr,
AnchorUSD, etc.), neither (1) nor (2) lies inside a window of a few
thousand recent ledgers. The account row gets created later via
side-effect referencing (the indexer hits an issuer in a payment op,
inserts the account with whatever fields it currently sees in the
trustline / balance change — `home_domain` is rarely there).

Result: `home_domain = NULL` for 99% of long-lived issuers on a fresh
backfill of a recent ledger window.

## Impact

- SEP-1 enrichment (`assets.icon_url`, classic credit `assets.name`)
  silently produces sentinel writes for 99% of classic credits on a
  small backfill window.
- The Type A audit (this task — 0197) cannot honestly verify SEP-1
  enrichment coverage on production-shape data without first solving
  this.
- Hides the real "Lambda 2 is wired correctly" signal under a flood of
  empty results — operator reads "9483 sentinel" and can't tell whether
  the worker is broken or just upstream-starved.
- Any production deploy that runs a similar window-limited backfill
  will see the same empty SEP-1 coverage and may incorrectly blame the
  enrichment worker.

## Proposed follow-ups

This warrants two separate BUG tasks:

### Task A — indexer should backfill account state on first observation

When the indexer encounters an account it has not seen before, it
should load the account's **current ledger entry** (via Stellar RPC
`getLedgerEntry` for `LedgerKey::Account`) and persist the full set of
account fields (`home_domain`, `sequence_number`, `inflation_dest`,
flags, signers, …), not just the subset that happens to appear in the
side-effect that introduced it.

Cost: one RPC call per never-before-seen account, cached afterwards.
The classic-credit producer follow-up (the other open finding) can
share this path.

### Task B — `accounts.home_domain` recompute-from-RPC, separate from indexer

Even with Task A in place, accounts created **before** the indexer
window via a side-effect path still have NULL fields. Add an
operator-driven backfill subcommand (parallel to
`backfill-enrichment-runner`) that walks accounts with
`home_domain IS NULL` and pulls the live RPC value. Idempotent.
Single-pass over the account table.

## Audit context

This is the second pre-audit finding (after the missing classic-credit
producer). Both together block audit 0197 from producing meaningful
empirical coverage numbers on a local mini-backfill. Once both are
fixed and re-runs of `backfill-runner` + `backfill-enrichment-runner`
produce production-shape data, the Type A coverage matrix from Step 1
can be filled in honestly.

## Raw queries (reproducible)

```sql
-- Empty home_domain on accounts seen as classic-credit issuers
SELECT COUNT(DISTINCT acc.id)
FROM accounts acc
JOIN account_balances_current abc ON abc.issuer_id = acc.id
WHERE abc.asset_code IS NOT NULL
  AND (acc.home_domain IS NULL OR acc.home_domain = '');

-- Coverage breakdown after sep1-assets drain on seeded assets table:
SELECT
  COUNT(*) FILTER (WHERE icon_url IS NULL)               AS transient_err,
  COUNT(*) FILTER (WHERE icon_url = '')                  AS sentinel_no_data,
  COUNT(*) FILTER (WHERE icon_url IS NOT NULL AND icon_url <> '') AS real_value,
  COUNT(*)                                               AS total
FROM assets WHERE asset_type IN (1, 2);

-- Of the empty-home_domain issuers, how many have classic-credit assets:
SELECT COUNT(DISTINCT (a.asset_code, a.issuer_id))
FROM assets a
JOIN accounts acc ON acc.id = a.issuer_id
WHERE a.asset_type = 1
  AND (acc.home_domain IS NULL OR acc.home_domain = '');
```
