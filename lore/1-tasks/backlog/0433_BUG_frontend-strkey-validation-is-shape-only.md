---
id: '0433'
title: 'BUG: frontend StrKey validation is regex shape-only — no checksum, and 2 of 3 test fixtures are fabricated'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0431', '0430']
tags: [priority-medium, effort-small, layer-frontend, correctness, validation]
links:
  - libs/ui/src/identifiers/validators.ts
history:
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      Found during the 0431 sweep for hand-rolled protocol logic. Same shape as
      0430: the implementation and its test share a blind spot, so the test
      confirms the bug instead of catching it.
      Verified independently — I implemented base32 + CRC16-XModem and ran it
      against the repo's own fixtures rather than trusting either the code or
      the agent that reported it.
---

# BUG: StrKey validation checks shape, not validity

## Summary

`libs/ui/src/identifiers/validators.ts` validates Stellar addresses with
regexes alone:

```ts
const STELLAR_ACCOUNT = /^G[A-Z2-7]{55}$/;
const STELLAR_CONTRACT = /^C[A-Z2-7]{55}$/;
const STELLAR_POOL = /^L[A-Z2-7]{55}$/;
```

A StrKey per SEP-23 is three things: a version byte, a base32 payload, and a
**CRC16-XModem checksum**. We check only that the string looks like base32 and
starts with the right letter. Any 56-character run of `[A-Z2-7]` passes.

## Evidence — the test fixtures are themselves invalid

Implemented base32 + CRC16-XModem independently and ran it over
`libs/ui/src/identifiers/validators.test.ts`:

| fixture                | our regex | real checksum                              |
| ---------------------- | --------- | ------------------------------------------ |
| `VALID_ACCOUNT`        | passes    | **FAILS** — computed 48885, embedded 32275 |
| `VALID_CONTRACT`       | passes    | passes                                     |
| `VALID_POOL`           | passes    | **FAILS** — computed 54196, embedded 29329 |
| `'G' + 'A'.repeat(55)` | passes    | **FAILS**                                  |

The test calls these "a valid CAP-23 G-strkey" and "a valid CAP-38 L-strkey".
Two of them are fabricated strings no Stellar node would accept. `VALID_POOL`
is visibly synthetic — the run `QX6FMT2BIPW5ELS` repeats inside it.

**This is the 0430 pattern again:** the assertion and the implementation share
the same blind spot, so the suite is green while the behaviour is wrong.

## Impact

Mostly pre-flight guards (14 call sites: account/contract/pool/asset/NFT detail
pages, transaction list, `useTxHashParam`), where a false accept costs one
doomed request the backend rejects anyway.

The real cost is user-facing at `web/src/pages/transactions/TransactionFilters.tsx:33`
and `web/src/pages/nfts/NftFilters.tsx:27`, which render an inline
"invalid address" hint. **A mistyped address with a broken checksum is exactly
what a checksum exists to catch — and we tell the user it looks fine.**

## Fix

`@stellar/stellar-base` exposes `StrKey.isValidEd25519PublicKey`,
`StrKey.isValidContract`, `StrKey.isValidLiquidityPool` — plus
`isValidMed25519PublicKey` (M…) and `isValidClaimableBalance` (B…), neither of
which we handle at all.

**But adding the dependency is a real decision, not a formality.** The frontend
deliberately has **zero** Stellar JS packages — verified across all five
`package.json` files and `package-lock.json`. That was a choice: task 0085
removed `@stellar/stellar-sdk` when the TS backend was replaced by Rust, and
task 0077 rejected the SDK for a single function on bundle-size grounds
("50–100 KB compressed"), hand-rolling a pool-id encoder instead — which was
later deleted once the backend served canonical values.

So there are three honest options:

1. **`@stellar/stellar-base`** (not the full SDK) for the checksum helpers —
   measure the bundle delta first; the precedent says this was rejected before.
2. **~20 lines of local base32 + CRC16-XModem**, tested against SEP-23's
   canonical vectors rather than invented strings. Contradicts
   [[feedback-prefer-official-library]], so it needs an explicit reason.
3. **Drop client-side validity checks entirely** and let the backend be the
   single authority — the guards are pre-flight anyway. Cheapest, and arguably
   most honest: one validator, server-side, already correct.

## Acceptance Criteria

- [ ] Decide among the three options above and record why.
- [ ] Test fixtures replaced with **real** addresses taken from mainnet (or
      SEP-23's own canonical vectors), never hand-typed.
- [ ] A deliberately checksum-broken address is rejected by the test suite.
- [ ] M… (muxed) and B… (claimable balance) handled or explicitly out of scope.
- [ ] Docs updated — `N/A`.
- [ ] API types regenerated — `N/A`.

## Note

`isAssetId` splitting `CODE-ISSUER` is legitimately ours — the SDK's canonical
separator is `:` and it ships no `CODE-ISSUER` parser. Only the embedded issuer
check inside it is affected.
