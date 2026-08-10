---
id: '0448'
title: 'RESEARCH: is compressing the ClickHouse insert body worth it once the write volume is fixed'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0447']
tags: [phase-future, effort-small, priority-low, clickhouse, cost, research]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Spawned from the July cost investigation. Typed inserts leave uncompressed
      and no setting changes that. Worth answering only after the dominant write
      volume is dealt with, hence a research task rather than a fix.
---

# Is insert-body compression worth it once volume is fixed

## Summary

The `clickhouse` crate compresses **responses** only. `src/insert.rs` contains no
compression path at all, unlike `src/insert_formatted.rs`, which compresses
explicitly. A typed `client.insert::<T>()` body therefore always leaves
uncompressed, and `enable_http_compression` — a server setting governing response
bodies — does not change that.

For a Lambda writing to ClickHouse over the public internet, every uncompressed
byte is billed egress. The question is whether that is worth engineering effort
**after** the write-volume problems are fixed, or whether it becomes noise.

## Why this is research and not a fix

Our own egress is ~3.4 GB/day, about **$9–10/month**. Compressing it might save a
few dollars. That is not worth a rewrite of the write path on its own.

It becomes interesting only if a large write volume is unavoidable. So the
sequencing is: fix volume first (0447 here, and the equivalent finding handed to
the prices-api team), then re-measure, then decide.

## Questions to answer

- What is our egress after 0447 and after any indexer batching changes? If it
  stays under ~$15/month, close this as not worth doing.
- Does a newer `clickhouse` crate version compress typed insert bodies? 0.15.0 is
  pinned exactly (`clickhouse = "=0.15.0"`). Check the changelog before assuming
  a rewrite is required.
- If a rewrite is required, what does `insert_formatted` cost us? It takes
  pre-formatted bytes rather than typed rows, so the typed-row ergonomics and the
  RBWNAT column header handling would have to be reproduced by hand.
- What compression ratio does our row shape actually achieve on the wire? On disk
  the store compresses 5.27 TiB to 1.06 TiB (~5:1), but that is columnar codecs
  over sorted parts; LZ4 over an unsorted RowBinary stream will do considerably
  worse. Measure, do not assume.

## Acceptance Criteria

- [ ] Post-0447 egress measured and stated in dollars per month
- [ ] Crate changelog checked for typed-insert compression support above 0.15.0
- [ ] A measured wire compression ratio for our actual row shape, not an estimate
- [ ] Explicit verdict: do it, or close as not worth the effort
