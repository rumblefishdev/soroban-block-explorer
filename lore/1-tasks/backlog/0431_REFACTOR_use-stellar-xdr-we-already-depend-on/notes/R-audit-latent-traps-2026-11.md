---
title: 'Latent decoding traps found by the 2026-11-17 audit'
type: research
status: seed
spawns: []
tags: [xdr, library]
links: []
history:
  - date: '2026-11-17'
    status: seed
    who: karolkow
    note: 'Findings 12–17 of the decoding audit; inputs for the per-helper verdict.'
---

# Latent decoding traps found by the 2026-11-17 audit

Findings 12–17 of the audit in
[0363 notes](../../../backlog/0363_FEATURE_advanced-scval-decoded-rendering/notes/R-audit-hand-rolled-decoding-2026-11.md):
three invalid-UTF-8 policies in one pipeline; `format_stroops` dropping the
sign below one unit; `decimal7_string_to_i128` reading `"5"` and `"5.0"` ten
million apart; `asset_code_str`'s `0x…` form colliding with real codes;
`parse_spec_entries` stopping silently on an unknown entry; and helpers that
duplicate `stellar-xdr` (`encode_xdr`, `pool_id_hex_to_strkey`,
`build_credit_asset`). None shows a wrong value in production today; each is a
row for this task's "adopt or keep ours with a reason" verdict.
