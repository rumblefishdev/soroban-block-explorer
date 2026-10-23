# Issue #684: metadata and image inlined as `data:` URIs (2026-10-21)

Contract
`CDC5ZX53PWDPVS5AGZPHJ6WHFPB66AZFFEHK4NCFH2AR7ZZQUKOGJFGB` (collection
`Stroopies`, 4 tokens): `token_uri` returns `data:application/json,{…}` —
plain, not `;base64` — and the JSON's `image` is itself an inline
`data:image/svg+xml,<svg …>` (~5 KB per token). `validate_uri` rejects the scheme, so
`nft_enrichment` holds empty `name` / `media_url` for all four (worker run
2026-10-20). Two consequences for A2: the decoder must take both the plain
and the `;base64` form, and a `data:image/…` value must be accepted as
`media_url`. The SPA's detail preview (`NftMediaPreview.tsx`, `classify`)
reads a file extension from the URL path; for this SVG it reads `4'/></svg>`
and shows "unsupported", so the detail page needs a `data:image/` branch too
(the list thumbnail renders any `src`).

The image is computed at call time, not stored. Footprint of `token_uri(0)`
from RPC simulation (2026-10-21): persistent `Vec[Member, 0]` and
`Vec[Art]`, two contract instances and two programs — the collection
contract reads the member's entry and calls a second (art) contract. So:

- the local executor of 0620 cannot answer it until persistent contract data
  is stored (task 0633);
- a value fetched once at mint can go stale when the member entry changes
  (the contract exports `set_role`, `set_bio`, promotions). Whether the
  image depends on those fields is not checked.

What the standards say (checked 2026-10-21):

- SEP-50 is Draft 0.1.0 and has no MUST/SHOULD at all. Its only words on the
  URI: `token_uri()` "returns an url that points to a JSON file" (rationale,
  sep-0050.md:373). No scheme, encoding or size rule, so an inline `data:` URI
  is unspecified, not forbidden; `image` is "a URI pointing to a resource with
  mime type image/\*", which `data:image/svg+xml` is.
- The plain form is not strictly RFC 2397 (raw `{ " <` and spaces are not
  URL characters) but the WHATWG data-URL processor accepts it; a raw `#` or
  `%XX` would corrupt it. The four Stroopies values carry neither.
- Stellar docs present `simulateTransaction` as the way to call read-only
  functions; `getLedgerEntries` returns raw entries and runs no code, so it
  cannot produce a computed `token_uri`.
- Freighter fetches metadata only over `https:` and has a test "rejects data:
  URIs" (`@shared/api/helpers/fetchMetadataJson.ts`). stellar.expert and
  Lobstr: not found in public code.

Measured 2026-10-23 (task 0655): 17.6% of a 148-token sample of stored-empty
NFTs now have data (12.8% `data:`, fixed by 0654; 4.7% links that now load),
so A2's retry of empty rows has a concrete target. Also: `--force-retry`
writes a full sentinel row on a permanent read failure, which replaces a good
stored row — a forced refill must not overwrite real values (A2/A3).
