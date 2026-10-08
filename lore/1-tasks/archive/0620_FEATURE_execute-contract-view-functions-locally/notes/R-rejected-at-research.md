# Rejected at research (task 0620)

- **Learn a per-program layout descriptor** (which key holds the value):
  reproduces 4,054 of 4,151 contracts, 0 mismatches, but needs the bytes and
  one initialised instance anyway, cannot be learned at upload (285 of 330
  programs trap on an empty instance), breaks on 27 cross-contract programs,
  and adds a second interpreter of storage that goes stale on upgrade.
- **Compute at upload:** only 45 of 330 token programs return decimals
  without state.
- **Cache by (program, storage hash):** 16% hit rate.
- **RPC simulation per contract:** an external dependency with rate limits
  (~50% of calls answered 429 during the census).
