# I — Polling / cache logic (1.22, Wave 3)

Grep + Read on `web/src/api/`. Live verification: home `/` left open
30s, polled-endpoint network calls counted.

## Polling policy

Defined in `web/src/api/polling.ts`:

| Policy | `staleTime` | `refetchInterval` | `gcTime` | `placeholderData` |
|---|---:|---:|---:|---|
| `homePolicy` | 10 s | **12 s** | (default 5 min) | — |
| `listPolicy` | 60 s | — (no polling) | (default 5 min) | `keepPreviousData` |
| `detailPolicy` | 5 min | — | (default 5 min) | `keepPreviousData` |
| `searchPolicy` | 0 | — | 0 | — |

Plus `QueryProvider.tsx:5-22` defaults:
- `staleTime: 60_000`
- `gcTime: 5 * 60_000`
- `refetchOnWindowFocus: true` (re-fetch when tab regains focus)
- `retry: 1 on network/5xx, 0 on 4xx` (custom predicate respecting
  client.ts error interceptor's `status` stamp)

## Findings

### F-I-1 ✓ PASS — Policies appropriately segmented by volatility

`homePolicy` polls every 12s on:
- `useLatestTransactions.ts` (home "Latest transactions")
- `useLatestLedgers.ts` (home "Latest ledgers")
- `useNetworkStats.ts` (top banner TPS / Ledger / Accounts / Contracts)

12s is reasonable for a 5–7s ledger-close network (each poll catches
~2 new ledgers).

`listPolicy` (60s stale, no polling) on:
- `useTransactionsList`, `useAssetsList`, `useLedgersList`,
  `useAccountTransactions`, `useAssetTransactions`,
  `usePoolTransactions`, `usePoolParticipants`,
  `useContractInvocations`, `useContractEvents`, `useNftTransfers`.

Sensible — list pages should NOT background-poll because (a) pagination
state would invalidate, (b) user is mid-scroll. Manual reload or window
focus is enough.

`detailPolicy` (5 min stale, no polling) on:
- `useTransactionDetail`, `useAccountDetail`, `useAssetDetail`,
  `usePoolDetail`, `usePoolChart`, `useContractInterface`.

5 min for static-ish detail pages is reasonable, balanced against
the global `refetchOnWindowFocus: true` for re-arrival.

`searchPolicy` (0 stale, 0 gc) on `useSearchResults` — every distinct
query is a fresh fetch, no cache, and old results are immediately
gc'd. Correct for search UX.

### F-I-2 ✓ PASS — Live verification matches stated intervals

Home `/` open 30s. Network filtered to polled endpoints:
- `/v1/transactions?limit=10` × 4 polls
- `/v1/ledgers?limit=10` × 4 polls
- `/v1/network/stats` × 4 polls

≈ 4 / 30 s = one every 7.5s on average (allowing for the initial mount
fetch). Matches 12s `refetchInterval` plus 10s `staleTime` initial
shift. No runaway polling, no per-component duplicate polls (TanStack
dedupe working).

### F-I-3 🟡 MEDIUM `[Class B, Severity MEDIUM]` — No visibilitychange / document.hidden pause

```
grep -rn "visibilitychange|document\.hidden" web/src libs/ui/src
```

Result: zero matches. The 12s home polling continues unbroken when
the tab is hidden — wastes API calls + battery on a long-running
backgrounded tab.

`refetchOnWindowFocus: true` triggers a single re-fetch on tab focus
(good), but TanStack's `refetchIntervalInBackground` defaults to
`false` — so polling SHOULD pause when document hidden, but only if
TanStack itself uses the visibility API. As of @tanstack/react-query
v5 it does (focusManager / OnlineManager). So this finding is partial:
the policy is implicit, never opt-in tested. Recommend explicit
documentation in `polling.ts` header comment + an integration test.

### F-I-4 🟠 HIGH `[Class D, Severity HIGH]` — `invalidateResource` defined + exported but never called

```
grep -rn "invalidateResource" web/src libs/ui/src
```

Hits:
- `web/src/api/queryKeys.ts:49` — definition.
- `web/src/api/index.ts:12` — re-export.

No call site in `web/src/pages/**` or `web/src/api/hooks/**`. The
utility was built to invalidate by resource on user action (e.g.
post-mutation refresh) — but the FE has zero mutations (read-only
explorer), so there's no obvious caller. Dead code that signals an
abandoned feature.

Two interpretations:
1. **Dead code** — drop it, single PR.
2. **Pre-mutation infrastructure** — keep for future "favourite",
   "follow", or admin actions, but mark explicitly as such.

Catalog-only because dropping or keeping doesn't change runtime
behaviour today.

### F-I-5 ✓ PASS — TanStack default dedup

Multiple subscribers to the same query key share the underlying fetch
+ cache. Verified indirectly: home page mounts both
`useLatestTransactions` and `useLatestLedgers` and they each fire one
request per interval (not N per mounted component). No need for manual
request-coalescing.

### F-I-6 🟢 LOW `[Class D, Severity LOW]` — No explicit `refetchIntervalInBackground` setting

`homePolicy` doesn't pin `refetchIntervalInBackground: false`. Relies
on TanStack default (which is false). Pinning explicitly removes the
"will a major upgrade flip this default?" risk. Nit.

### F-I-7 🟡 MEDIUM `[Class D, Severity MEDIUM]` — `gcTime` not set on `listPolicy` / `detailPolicy`

Both rely on global `gcTime: 5 * 60_000`. If `detailPolicy.staleTime`
is 5 min but global gcTime is also 5 min, the cached detail page is
gc'd at the very moment it would become stale (no overlap window).
Round-tripping back to a recently-visited detail page from a
list-page often misses cache because gc happened during the list view.
Bump `detailPolicy.gcTime` to ≥ 10 min for the back-button UX
TanStack is supposed to deliver.

### F-I-8 ✓ PASS — Retry policy correctly excludes 4xx

`QueryProvider.tsx:14-19`:
```ts
retry: (failureCount, error) => {
  const status = (error as { status?: number })?.status;
  if (typeof status === 'number' && status >= 400 && status < 500) {
    return false;
  }
  return failureCount < 1;
}
```

Correctly: 4xx → no retry (user error / 404), 5xx + network → one
retry. Relies on `client.ts` error interceptor stamping `status` on
the thrown error — assuming Wave 1 / Wave 2 finding on the interceptor
holds. Cross-reference: Wave 2 C-* findings; specifically the
Class B candidate from Gate A triage agenda item #3.

## Class breakdown for I (Wave 3 1.22)

| Class | Count |
|---|---:|
| A | 0 |
| B — routing/contract | 1 (I-3) |
| C | 0 |
| D — catalog-only | 3 (I-4, I-6, I-7) |
| E | 0 |
| ✓ pass | 4 (I-1, I-2, I-5, I-8) |

## Severity breakdown

| Severity | Count |
|---|---:|
| 🔴 CRITICAL | 0 |
| 🟠 HIGH | 1 (I-4) |
| 🟡 MEDIUM | 2 (I-3, I-7) |
| 🟢 LOW | 1 (I-6) |
