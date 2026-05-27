# API URL Conventions

Per [CAP-38](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0038.md)
and SEP-23, all Stellar / Soroban entity identifiers in user-facing
contexts use **strkey canonical form**. Hex, numeric, and polymorphic
forms are documented exceptions where the entity type is not
strkey-eligible by protocol.

This document is the single source of truth for the path-parameter
format accepted by every public REST endpoint and the matching URL
builder on the frontend.

## Per-endpoint path parameter formats

| Endpoint                          | Path param      | Form                       | Validator                          | Rationale                                                                                                                                                                                                                      |
| --------------------------------- | --------------- | -------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/v1/accounts/:id`                | account ID      | strkey `G...` (56 chars)   | `common::path::strkey(_, 'G', _)`  | CAP-38 canonical account ID.                                                                                                                                                                                                   |
| `/v1/contracts/:id`               | contract ID     | strkey `C...` (56 chars)   | `common::path::strkey(_, 'C', _)`  | CAP-38 canonical contract ID.                                                                                                                                                                                                  |
| `/v1/liquidity-pools/:id`         | pool ID         | strkey `L...` (56 chars)   | `common::path::pool_id_strkey`     | SEP-23 / CAP-38 canonical pool ID. DB storage is `BYTEA(32)` (raw hash, per ADR 0024); validator decodes strkey at the boundary and returns hex for downstream lookup.                                                         |
| `/v1/nfts/:contract_id/:token_id` | NFT instance    | strkey `C...` + `token_id` | `nfts::handlers::parse_nft_path`   | NFT = `(contract, token_id)` composite. stellar.expert addresses Soroban tokens via the contract URL; no numeric NFT route exists in the ecosystem. Internal `nft_id i32` surrogate PK is kept internal-only (cursors, joins). |
| `/v1/assets/:id`                  | asset ID        | polymorphic                | `assets::handlers::parse_asset_id` | Numeric `assets.id` (internal surrogate) OR strkey `C...` (Stellar Asset Contract) OR `code-issuer` composite (classic asset). Documented exception — three forms accepted by design.                                          |
| `/v1/transactions/:hash`          | tx hash         | hex 64-lower               | `common::path::parse_hash`         | Tx hash is raw bytes per Stellar protocol; no strkey form exists.                                                                                                                                                              |
| `/v1/ledgers/:seq`                | ledger sequence | numeric `u32`              | `common::path::sequence`           | Ledger sequence is a counter, not an identifier.                                                                                                                                                                               |

## Frontend URL builder conventions

`web/src/router/routes.ts` is the single owner of URL construction.
Every builder takes the canonical form of its target entity:

- `routes.account(strkey)` — strkey `G...`
- `routes.contract(strkey)` — strkey `C...`
- `routes.pool(strkey)` — strkey `L...`
- `routes.nft(contractId, tokenId)` — composite, contract strkey `C...` + opaque `token_id`
- `routes.asset(id)` — accepts any of the three asset forms accepted by the API
- `routes.transaction(hash)` — hex 64-lower
- `routes.ledger(seq)` — numeric

No hex inputs are accepted on the FE side except for transaction
hashes. Internal storage (DB hex for pool IDs, surrogate PKs for NFTs)
never appears in URLs or response wire fields.

## Cross-entity link integrity

Every clickable identifier in the UI links to its detail page using
the canonical form documented above. The audit baseline lives under
task 0257 Wave 3 1.7 ("cross-entity link integrity") in
`lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/`.

## Search input

The global search bar accepts the same canonical forms as the path
parameters. The empty-state hint in `web/src/search/SearchResultsView.tsx`
enumerates the supported prefixes (`G…`, `C…`, `L…`, hash, token code)
and MUST stay in sync with this document.

### Direct redirect — backend (`/v1/search` `SearchResponse::Redirect`)

When `q` is a fully-typed entity id, the `Classified` struct produced
by `crates/api/src/search/classifier.rs` returns `true` from its
derived `is_fully_typed()` method, and `fetch_redirect`
(`crates/api/src/search/queries.rs`) performs an exact-match lookup.
On hit, the response is `SearchResponse::Redirect`; on miss, the broad
search runs. The derived predicate is `true` when `hash_bytes` is
populated or when `strkey_prefix` is exactly 56 chars long. The
classifier covers four deterministic shapes:

| Input shape              | `Classified` channel       | Redirect target         |
| ------------------------ | -------------------------- | ----------------------- |
| 64-hex                   | `hash_bytes`               | `/transactions/<hash>`  |
| full L-strkey (56 chars) | `hash_bytes` (via decode)  | `/liquidity-pools/<L…>` |
| full G-strkey (56 chars) | `strkey_prefix` (56 chars) | `/accounts/<G…>`        |
| full C-strkey (56 chars) | `strkey_prefix` (56 chars) | `/contracts/<C…>`       |

Partial G-/C- prefixes (`is_strkey_prefix`) drive the prefix CTEs in
broad search and are not eligible for redirect — only full 56-char
strkeys redirect. Partial L-prefix is not classified at all; the pool
table stores raw `BYTEA(32)` with no text mirror column (tracked in
backlog task 0271).

### Direct redirect — frontend (`directRouteFor`)

`web/src/search/directRouteFor.ts` handles inputs the backend redirect
path doesn't cover. Today the only case is a bare-digit ledger
sequence — ledger is not a broad-search bucket entity, so the FE
short-circuits before calling `/v1/search`.

| Input shape      | Redirect target  |
| ---------------- | ---------------- |
| bare-digit `u32` | `/ledgers/<seq>` |

`directRouteFor` is invoked at every search entry point so the
behaviour is consistent regardless of how the query reaches the app:

- `web/src/router/AppShell.tsx` `handleSearchSubmit` — global search
  bar submit (typed + Enter).
- `web/src/pages/home/HomeHero.tsx` `submit` — home-page hero search
  submit.
- `web/src/pages/SearchResultsPage.tsx` `useEffect` on `q` — handles
  deep-link `/search?q=<digits>` and typing into the dedicated
  search page's own `SearchInput` (which writes back to the URL
  `q` param).

On null return, callers fall through to `routes.search(q)` and let
the backend resolve the input.

## Why this matters

Stellar Expert, Horizon, Stellar Lab, Soroban CLI, and the broader
Stellar SDK ecosystem use strkey for all human-facing identifiers.
External "paste-from-explorer-URL" scenarios must work without manual
conversion. Project alignment with ecosystem convention reduces
onboarding friction and avoids surprising users.

## Maintenance

Tracked under [ADR 0032](../../../lore/2-adrs/0032_docs-architecture-evergreen-maintenance.md)
(evergreen architecture docs gate) — update this file whenever any
endpoint accepts or emits a new identifier shape, a new public route
is added, or a path validator changes.

Cross-references:

- ADR 0008 — strkey adoption
- ADR 0024 — liquidity-pool storage shape
- Task 0264 — strkey canonical everywhere refactor (originated this doc)
- `crates/api/src/common/path.rs` — shared path validators
