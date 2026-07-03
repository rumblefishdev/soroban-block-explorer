# SCF Milestone 2 - Form Answers

Copy the text inside each field into the matching SCF form field.

## Field 1 - Tranche Deliverables

> **Deliverable 2 - Complete API + Frontend.**
>
> Milestone 2 delivers the public React explorer and complete REST API for
> Stellar mainnet data. The frontend is available at
> `https://sorobanscan.rumblefish.dev` and the API is available at
> `https://api-sorobanscan.rumblefishdev.com/v1`.
>
> What is live and verifiable:
>
> 1. **REST API endpoints serving mainnet data:** transactions (list + detail),
>    ledgers (list + detail), accounts (detail + history), contracts (detail +
>    invocations + events), assets, NFTs, liquidity pools, and search.
> 2. **Schema-valid API responses:** the deployed API is documented through
>    Swagger / OpenAPI at
>    `https://api-sorobanscan.rumblefishdev.com/api-docs`. The evidence package
>    includes successful API responses for reviewer-provided mainnet entity IDs.
> 3. **Decoded Soroban invocations:** for at least three known contract
>    transactions, the decoded call detail - function name, decoded arguments,
>    and decoded return value (not raw XDR) - is shown on the Transaction Detail
>    Advanced view, reached from the contract's Invocations tab, and is returned
>    by the transaction-detail API.
> 4. **Decoded CAP-67 events:** Transaction Detail pages include an Events tab
>    where CAP-67 topics and data are decoded into readable fields. The matching
>    transaction detail API response exposes the same decoded event data.
> 5. **Global search:** exact transaction hash, account ID, and contract ID
>    searches route to the correct detail pages.
> 6. **Public React frontend:** the application renders live mainnet data across
>    all top-level pages and representative detail pages: Transactions, Ledgers,
>    Accounts, Contracts, Assets, NFTs, and Liquidity Pools.
> 7. **Edge protection and caching:** the API is served through the
>    Cloudflare-fronted host and is access-controlled; an API key is available
>    to reviewers on request. Rate limiting and response caching are configured
>    on the API path.
>
> Full evidence package:
> https://drive.google.com/drive/folders/1MjTtdFkYGfp_txRSgdxWKP0fra7haHov?usp=share_link

## Field 2 - Deliverable Verification - Video

> https://drive.google.com/file/d/1EHt59BskrLN-zLXcWDSKXwd8PEfjgpCp/view?usp=share_link

## Field 3 - Additional Deliverable Verification

> **Evidence package:** https://drive.google.com/drive/folders/1MjTtdFkYGfp_txRSgdxWKP0fra7haHov?usp=share_link - contains the Milestone 2
> verification video, `milestone-2-evidence.pdf`, screenshots, and API response
> files for each acceptance criterion.
>
> **Live application:**
>
> - Frontend: `https://sorobanscan.rumblefish.dev`
> - API base: `https://api-sorobanscan.rumblefishdev.com/v1`
> - Swagger UI: `https://api-sorobanscan.rumblefishdev.com/api-docs`
> - OpenAPI JSON: `https://api-sorobanscan.rumblefishdev.com/api-docs-json`
>
> **Reviewer API access:** the API is access-controlled and is demonstrated in
> the verification video; an API key (`x-api-key` header) is available to
> reviewers on request.
>
> **Source code:**
>
> - Repository: `https://github.com/rumblefishdev/soroban-block-explorer`
> - Technical design:
>   `https://github.com/rumblefishdev/soroban-block-explorer/blob/master/docs/architecture/technical-design-general-overview.md`
> - API implementation:
>   `https://github.com/rumblefishdev/soroban-block-explorer/tree/master/crates/api/src`
> - Frontend implementation:
>   `https://github.com/rumblefishdev/soroban-block-explorer/tree/master/web/src`

## Field 4 - Support Needed

> -
