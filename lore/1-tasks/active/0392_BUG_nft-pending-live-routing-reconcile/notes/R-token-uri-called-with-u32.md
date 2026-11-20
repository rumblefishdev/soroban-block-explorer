---
title: 'token_uri is always called with a u32 token id'
type: research
status: seed
spawns: []
tags: [nft, enrichment]
links: []
history:
  - date: '2026-11-17'
    status: seed
    who: karolkow
    note: 'From the 2026-11-17 decoding audit (0363 notes, finding 7); not yet measured.'
---

# token_uri is always called with a u32 token id

`token_id_to_string` (`crates/xdr-parser/src/state/nfts.rs`) flattens the token
id to its bare value: u32 `5`, u64 `5`, i128 `"5"` and string `"5"` all become
`"5"`. The enrichment then parses it as `u32`
(`crates/enrichment-shared/src/nft_token_uri/client.rs:318`) and calls
`token_uri` with `ScVal::U32` (`:489`). A contract whose token id is u64, i128
or a string gets a mistyped argument; the host rejects it, the URI never
resolves and the NFT has no media. How many contracts use a non-u32 id is not
measured yet.
