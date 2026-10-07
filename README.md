# Soroban Block Explorer

[**Backlog Board**](https://rumblefishdev.github.io/soroban-block-explorer/)

Nx + TypeScript monorepo bootstrap for a Soroban-first Stellar block explorer.

This repository starts from the official `nrwl/typescript-template` foundation and adapts
it to the planned product architecture:

- `web` for the frontend explorer UI
- `crates/api` for the public REST API (Rust/axum)
- `crates/indexer` for ledger ingestion entrypoints (Rust)
- `infra` for infrastructure as code (AWS CDK)
- `libs/ui` for shared frontend components

## Quick Start

```bash
nvm use
pnpm install
pnpm run lint
pnpm run build
pnpm run typecheck
```

## Workspace Layout

```text
web/
crates/
  api/
  indexer/
  xdr-parser/
infra/
libs/
  ui/
docs/
  architecture/
```

## Current Status

The workspace contains:

- root Nx / TypeScript / ESLint / Prettier bootstrap
- `web` — React 19 + Vite SPA with MUI, React Router, and TanStack Query
- `libs/ui` — shared React component library (Vite lib mode)
- `infra` — AWS CDK infrastructure stacks
- `crates/` — Rust backend (api, indexer, xdr-parser)
- architecture docs aligned with the reviewed technical design

Backend: Rust (axum + utoipa + sqlx), deployed as Lambda via cargo-lambda (per ADR 0005).
They will be introduced as dedicated follow-up steps.

## Soran names on Testnet

Testnet builds (`VITE_STELLAR_NETWORK=testnet`) resolve Soran names automatically
after 350 ms of typing, or immediately on Enter, and show elected primary names
beside account and contract IDs. Names such as `alice.nova` and `pay.alice.nova`
use open namespaces, so ordinary search results remain available for dotted
queries. Resolved names appear in the existing Accounts or Contract tab, with native
counts and selection. Duplicate indexed destinations become one row. Results
preserve the full destination and required memo. Muxed addresses
remain visible and copyable without a misleading account-page link.

Account links retain the searched name in the `soran` query parameter. The
account summary resolves that name again and shows its required memo only when
the complete destination matches the displayed account. Direct account visits
use the elected primary name when available. Payment details stay associated
with their name, so aliases sharing an account can retain different memos;
URL parameters never supply trusted memo values.

Resolution uses `@stellar/stellar-sdk` 17.0.1 and unsigned, read-only
`simulateTransaction` calls to `https://soroban-testnet.stellar.org`. It does not
use the Soran HTTP API or Soran SDK, and never signs or submits a transaction.
No wallet, funds, session, or API key is needed for these reads. Mainnet builds
make no Soran RPC requests. One 15-second deadline covers the entire lookup;
changing the query cancels its pending requests.

The pinned Testnet deployment is:

| Contract | Address                                                    |
| -------- | ---------------------------------------------------------- |
| Lookup   | `CDSORANQAJK35UV2HR63CMB6M5NYISHMUBTB6EQY2CZ3Y7HJDIOHRJWA` |
| Registry | `CCSORANDPQINYOYB5SVO45WJP2LBBYKC72HHUIRVXB4J6RUZKDAUW7G4` |
| Primary  | `CCSORAN7Y7ICQK2MBSVCJT3BUN5EHXKDKSTMGVB6QWSYXWMMLG2WIFJ6` |

Before calling Lookup's `resolve_destination`, the explorer checks its registry
anchor and Lookup / destination ABI versions (both 2). Child names additionally
require subname ABI version 1. Lookup checks current routing, ownership, expiry,
generation, and payment metadata in its resolution call. The explorer strictly
decodes destination ABI 2: G accounts retain any required memo, C contracts must
have no memo, and M addresses retain their full unsigned 64-bit ID. Unsupported,
malformed, unavailable, or archived responses fail without a legacy fallback.

Primary labels use Lookup's on-chain `primary_name` after verifying its Primary
anchor. Full M identities use `primary_name_muxed` with both the G account and
u64 ID, gated by muxed identity ABI version 1. These methods verify the election
against the current resolver. Empty or failed reads omit the optional label;
the original identifier always remains visible. No holder-name hint is used.

These checks trust the public RPC response and Soran's upgradeable contracts;
they do not pin contract code or independently prove ledger consensus. Separate
capability and resolution simulations may observe different ledgers. See
[Soran deployment metadata](https://github.com/SoranDomains/docs/blob/main/reference/deployments/testnet.json),
[the Lookup client reference](https://github.com/SoranDomains/sdk/blob/main/packages/lookup/src/index.ts),
and the [reference explorer integration](https://github.com/flarcos/stellar-expert-explorer/pull/1).

Requests omit cookies and referrers, but the public RPC operator receives the
viewer's IP and queried names or addresses, including automatic primary lookups
on detail pages. When shipping this integration to the public explorer, maintainers
should review the privacy disclosure with the policy's author; the policy is
maintained as author-supplied text in `PrivacyPolicyPage.tsx`.

To preview locally against Testnet:

```sh
VITE_STELLAR_NETWORK=testnet \
VITE_API_BASE_URL=https://api-testnet-sorobanscan.rumblefishdev.com \
VITE_TURNSTILE_SITE_KEY=0x4AAAAAADh96AKvSoW5C7CO \
pnpm web:serve
```

Then open `http://localhost:4200/search?q=robert.nova` (a live example that can
change). The explorer API requires a Turnstile session; if its site key does
not authorize your local host, use the project's documented dev API proxy
with an authorized dev key. Soran's public RPC lookup needs no session. Unit
tests use fixed responses, so they do not depend on live names or API access.

## Infrastructure

AWS infrastructure is managed with CDK (TypeScript) in `infra/`.

### Prerequisites

- AWS CLI configured with a named profile:
  ```bash
  aws configure --profile soroban-explorer
  ```
- Set the profile in your shell:
  ```bash
  export AWS_PROFILE=soroban-explorer
  ```

### First-time setup

Bootstrap CDK on the AWS account (once per account + region):

```bash
pnpm run infra:bootstrap
```

### Deploying

**See [`docs/deployment.md`](docs/deployment.md) — the single source of truth**
for what command ships what. Production is the only environment.

A release is a git tag: pushing `production-YYYY.MM.DD-N` to `master` runs
[`.github/workflows/deploy-production.yml`](.github/workflows/deploy-production.yml)
— `cdk diff` → deploy Compute → sync the SPA → smoke tests.

```bash
git tag production-$(date +%Y.%m.%d)-1 master && git push origin --tags
```

> The manual laptop path stays available, and is the one to use for surgical,
> single-stack deploys:

```bash
make -C infra diff-production              # preview
make -C infra deploy-production-compute    # ship the API / indexer / enrichment Lambdas
make -C infra deploy-production-web        # ship the frontend SPA
make -C infra deploy-production            # deploy ALL stacks (see gotchas in the guide)
```

> There is **no staging environment**. AWS staging was retired by task 0249,
> and the `deploy-staging.yml` / `scripts/staging-deploy.sh` fossils by 0390.
> `pnpm run infra:*:staging` and any `make deploy-staging*` target are **dead**
> — they reference targets that no longer exist. Do not use them.
