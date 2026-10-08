# Partly un-refuted — the clobber is real, but it is not the main cause

The `accounts` table is a ReplacingMergeTree and its own schema comment warns
that reads must take the latest version, so a whole-row clobber looked like
the obvious cause: a batch that sees the account only as a participant writes
a fresh row with a bumped `last_seen_ledger` and a null domain, and the newest
row wins.

**That measurement said zero. It is wrong** — corrected 2026-09-03 while
shipping [[0443]], which reads `home_domain` on two pages and made the
disagreement visible.

A verified counterexample, `GA22QHQPZVMQDBOCTOETGVAZBDKTPTK3STYQF5AEQEOFT6TATU6AFWY3`:

| ledger     | closed     | `home_domain`          |
| ---------- | ---------- | ---------------------- |
| 63 407 950 | 2026-07-10 | `coollifeclc.xmint.io` |
| 63 635 833 | 2026-07-25 | `NULL`                 |

`accounts FINAL` returns `NULL`. The chain, asked directly via
`getLedgerEntries` and decoded with the official `stellar xdr` CLI, returns
`coollifeclc.xmint.io`. The clobbering row predates the 2026-08-10
measurement, so this is not drift since — the earlier count was simply wrong.

Reproduce (per prefix slice — a full-table `GROUP BY` exceeds the read memory
limit):

```sql
SELECT count() FROM (
  SELECT id FROM accounts WHERE account_id LIKE 'GA22%' GROUP BY id
  HAVING maxIf(last_seen_ledger, home_domain IS NULL)
       > maxIf(last_seen_ledger, home_domain IS NOT NULL)
     AND countIf(home_domain IS NOT NULL) > 0)
```

**Scale, and why the hypothesis below still stands.** Three slices measured
2026-09-03: `GA22%` 13, `GB33%` 6, `GC44%` 6 — 25 accounts in roughly 11 700,
about 0.2%. Every one of the 13 in the first slice was checked against the
chain: **13 of 13 still carry the domain there**, so none is a user clearing
it. But 0.2% cannot explain a 22.2% coverage figure. The clobber is a real
second mechanism, not the explanation — treat it as a correctness bug in its
own right (it belongs to the class audited by [[0316]]) and keep the leading
hypothesis below as the answer to the coverage question.

**Consequence today, user-visible.** `crates/api/src/accounts/queries.rs`
reads with `FINAL` and serves `null`; the transaction-detail path added by
[[0443]] reads with `argMax(home_domain, last_seen_ledger)`, which skips the
NULL and matches the chain. The same account therefore shows its federated
address on a transaction page and not on its own account page. `argMax` is a
read-side patch, not a fix: it cannot distinguish "this batch did not carry
the field" from "the user cleared it", because the writer
(`crates/db-clickhouse/src/persist/stage.rs:838`,
`home_domain: ov.and_then(|o| o.home_domain.clone())`) writes `NULL` for both.
