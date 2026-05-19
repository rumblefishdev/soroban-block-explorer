l# Serverless at the Edge of a Blockchain: Architecture Decisions Behind Our Soroban Explorer

Earlier this year, Rumble Fish received a $131,200 grant from the Stellar Community Fund to build a Soroban-first block explorer — a publicly accessible tool for navigating transactions, smart contracts, accounts, and events on the Stellar network. The grant comes with a public mandate: the repository is open source, the infrastructure is reproducible, and the whole thing has to work reliably at mainnet scale.

Block explorers sound like a solved problem. They are not, at least not for Soroban. Soroban is Stellar's smart contract platform, and it produces data structures that existing explorers don't fully surface: decoded contract invocation arguments, return values, nested call trees, and CAP-67 contract events. Beyond that, the Stellar protocol produces a new binary payload — a `LedgerCloseMeta` XDR file — every five to six seconds, containing the complete record of every change that occurred in that ledger close. You need to ingest it in near-real time, decode it, store it, and serve it through a public API before the next one arrives.

This article walks through the decisions that shaped our architecture. Some of them are conventional. A few are not. We replaced a planned 7 TB PostgreSQL database with a 900 MB ClickHouse instance. We run our backend Lambdas in Rust. We ran the historical backfill on local machines because that was the right tool for that specific job. Here is how we got there.

---

## Why Our Live Pipeline Doesn't Call Any External Chain API

The straightforward approach to building a block explorer is wrapping an existing one. Horizon has been the standard Stellar API for years. Soroban RPC provides contract-level access. Both are options.

We rejected them for the live ingestion and serving path.

**Horizon is deprecated** for the indexing use cases we need. The Stellar Foundation's own tooling has moved beyond it, and building on a deprecated API means inheriting its limitations permanently.

**Rate limits are fine until they're not.** A block explorer that depends on another service's API inherits that service's reliability ceiling. Traffic spikes on the Stellar network — exactly when users want a block explorer most — are also when upstream APIs are most likely to throttle requests.

**Soroban-specific data isn't pre-assembled anywhere.** We need decoded contract invocation trees showing nested contract-to-contract calls. We need function arguments and return values decoded from their binary `ScVal` representation into structured, readable data. We need function signatures extracted from the WASM bytecode at contract deployment time. None of this exists in a form we can simply fetch from an existing API.

The live path — Galexie → S3 → Ledger Processor → ClickHouse → API — is fully self-contained. Everything the API serves comes from our own database, populated from canonical `LedgerCloseMeta` data alone. We do make targeted use of external calls in other parts of the system: the enrichment worker fetches `stellar.toml` files for asset metadata, and the historical backfill uses Soroban RPC `getLedgerEntries` to bootstrap account states for accounts referenced in a backfill window but never mutated within it. Those are bounded, offline or asynchronous operations — not dependencies woven through the live serving path.

---

## One Monorepo, Multiple Languages — The Nx Choice

The workspace spans over a dozen packages across TypeScript and Rust. The main ones:

| Package                    | Language   | Role                                                |
| -------------------------- | ---------- | --------------------------------------------------- |
| `crates/indexer`           | Rust       | Ledger Processor Lambda — XDR ingestion             |
| `crates/enrichment-worker` | Rust       | Enrichment Lambda — asset metadata & NFT resolution |
| `crates/api`               | Rust       | REST API Lambda                                     |
| `crates/xdr-parser`        | Rust       | XDR parsing library shared by ingestion crates      |
| `crates/domain`            | Rust       | Shared domain types                                 |
| `crates/db`                | Rust       | ClickHouse data access layer                        |
| `crates/enrichment-shared` | Rust       | Shared enrichment logic                             |
| `web/`                     | TypeScript | React SPA                                           |
| `infra/`                   | TypeScript | CDK infrastructure                                  |
| `libs/api-types`           | TypeScript | Shared API type contracts                           |
| `libs/ui`                  | TypeScript | Reusable React components                           |

These are not independent microservices that happen to live in the same repository. They are genuinely coupled. The domain types the frontend uses to render a transaction are the same concepts the indexer writes to the database. A change to how we store Soroban invocations touches both the Ledger Processor and the API. The CDK stack deploys all three Lambdas and knows their artifact paths. The coupling is real — the only question is whether to make it visible or hide it across repository boundaries.

We chose [Nx](https://nx.dev) to make it visible.

With Nx, `nx affected --target=build` builds only the packages touched by a given change. On a workspace spanning two languages and over a dozen packages, this matters: a change to `libs/ui` should not trigger a Rust compilation. `nx run-many -t typecheck` runs TypeScript type checking across all TypeScript packages in one command. The project dependency graph is explicit and visualisable — you can see at a glance that `crates/api` depends on `crates/domain`, and that `infra/` depends on nothing in the application layer but deploys everything.

The alternative — one repository per service — would have distributed the coupling across network boundaries rather than eliminated it. You would end up with shared type packages, inter-repo version pinning, and deployment coordination scripts that reinvent what a monorepo gives you for free. We have seen that pattern enough times to prefer the explicit version.

---

## Why We Write Our Lambdas in Rust

Rumble Fish has historically written backends in TypeScript. It is ergonomic, the ecosystem is extensive, and getting a new engineer productive in a Node.js Lambda takes hours, not weeks. For this project we made a different choice: all three backend Lambda functions are Rust crates. Here is why.

### Cold start performance

Lambda cold starts are a function of two things: how large the deployment artifact is and how expensive the runtime initialisation is. A TypeScript Lambda ships a Node.js runtime plus your application code plus its `node_modules` — commonly several megabytes, sometimes significantly more. A compiled Rust Lambda is a single statically-linked binary, typically under five megabytes, with no runtime to initialise. It starts in milliseconds.

For the Ledger Processor, which fires on every S3 event — roughly 17,000 times a day — provisioned concurrency is not a practical solution. Provisioned concurrency keeps instances warm, but it makes most sense for steady-state traffic, not for a function that needs to respond to an event every five seconds indefinitely. A fast cold start is preferable to paying for warmth.

### Execution speed

XDR parsing is not lightweight. Each `LedgerCloseMeta` payload contains every transaction, every operation, every Soroban invocation, every CAP-67 event, and every ledger entry change for a complete ledger close. For an active ledger that can mean thousands of decode operations: binary deserialisation, tree traversal, type-tagged value decoding. Rust does this with predictable memory usage, no garbage collection pauses, and throughput that is roughly an order of magnitude higher than equivalent Node.js.

Our target is under ten seconds from ledger close to database write. With the Ledger Processor in Rust, we are well inside it with headroom.

### The AI development loop

This is the argument we did not expect to be making when the project started.

The Rust compiler is famously strict. Ownership violations, type mismatches, and lifetime errors all surface at compile time with precise, often instructive messages. This turns out to be highly productive when working with AI-assisted development: the agent writes code, the compiler rejects it with a specific error, the agent reads the error and corrects it. The loop is fast and deterministic. There is no class of "it ran but produced the wrong output" error that requires runtime investigation.

With TypeScript or Python, AI-generated code can be syntactically valid and dynamically typed in ways that fail silently at runtime or produce subtly wrong results. With Rust, the compiler acts as a continuous integration step on every compilation. Errors are loud, early, and actionable.

As AI tooling has made Rust more accessible — removing much of the manual borrow-checker wrestling that historically made it a steep climb — the performance and correctness benefits have become easier to capture. We now consider it the better default for backend Lambda work, and this project is the system that shifted our thinking.

AWS supports Rust as a first-class Lambda runtime. The `lambda_runtime` crate, per-service AWS SDK crates (`aws-sdk-s3`, `aws-sdk-sqs`, and so on), and the `cargo-lambda` build toolchain cover everything needed from local development to deployment.

---

## The Ingestion Pipeline — Matching Compute to Workload Shape

The ingestion pipeline has three distinct jobs. We use three different execution models for them, because the workloads have different shapes and applying one model to all three would have been a mistake.

```
Stellar peers
  → Galexie (ECS Fargate, continuous)
  → S3: stellar-ledger-data/ (~1 file per ledger, zstd-compressed XDR)
  → Ledger Processor Lambda (Rust, S3 PutObject trigger)
  → ClickHouse (Hetzner)
       ↑
  Enrichment Worker Lambda (Rust, SQS-driven)

API Gateway → API Lambda (Rust) → ClickHouse (Hetzner)
CloudFront  → React SPA
```

### Galexie on ECS Fargate

Galexie is the Stellar Foundation's tool for streaming canonical ledger data. It connects to Stellar network peers via an embedded Captive Core process, maintains a persistent connection, and exports one `LedgerCloseMeta` XDR file to S3 for each ledger close — roughly every five to six seconds, indefinitely.

Lambda is the wrong shape for this. Galexie is a long-running, stateful process. It maintains a peer connection across ledger closes, tracks which ledger it last exported, and resumes from that checkpoint on restart. Lambda has a fifteen-minute execution limit and no concept of resumption. ECS Fargate gives us a managed container runtime — one task, always running, no EC2 to provision or patch.

### Lambda (Rust) for the Ledger Processor

When Galexie writes a file to S3, an S3 PutObject notification triggers the Ledger Processor Lambda. This is the primary ingestion worker, and it is exactly the shape that Lambda is designed for: one event, one bounded unit of work, invoked per ledger close, completing in well under ten seconds.

The Lambda downloads and decompresses the XDR file, parses every entity it contains, and writes structured records to ClickHouse in a single operation. If the Lambda fails, Lambda retries automatically. If there is a permanent failure, the file remains in S3 — we can replay any ledger by re-triggering the Lambda with its S3 key.

That replayability is the reason S3 is the handoff between Galexie and the Ledger Processor, rather than a direct invocation or a queue. A durable artifact per ledger close means the ingestion pipeline can recover from failures, schema migrations, or bugs in the parsing logic by replaying affected ledgers without re-ingesting from the network.

### Lambda (Rust) for the Enrichment Worker

A third Lambda handles metadata enrichment, driven by an SQS queue rather than a scheduled trigger. When the Ledger Processor writes new records, it enqueues enrichment work; the enrichment worker picks it up and fills in data that is not present in `LedgerCloseMeta` itself.

Concretely, this means two things: fetching asset metadata from `stellar.toml` files via SEP-1 — the Stellar ecosystem standard for publishing token information — and resolving NFT token URIs to pull in off-chain media and attribute data.

This lives in a separate Lambda from the Ledger Processor deliberately. Canonical chain data and off-chain metadata have different reliability profiles: `LedgerCloseMeta` is deterministic and always available; a `stellar.toml` endpoint may be slow, absent, or return incomplete data. Separating them means a slow external fetch never stalls the ingestion path, and enrichment can be retried independently if it fails.

---

## The Historical Backfill — When Architecture Meets Reality

Soroban launched on Stellar mainnet in late 2023. Our explorer needs to surface data from the beginning, which means indexing roughly two years of history before going live. The design called for a separate ECS Fargate task to read from Stellar's public history archives and feed data through the same S3 → Lambda → ClickHouse pipeline as live ingestion — one code path for both problems, clean separation, no special cases.

We did not do that.

We ran the backfill on local machines, writing directly to a local ClickHouse instance. Once the backfill was complete, we migrated the data to production ClickHouse on Hetzner.

Two reasons: speed and cost.

The event-driven pipeline is optimised for steady, continuous work — one ledger at a time, one S3 event at a time, with the overhead of Lambda invocations, S3 round-trips, and network hops to the database. That overhead is trivially small for live ingestion where a new ledger arrives every five seconds. For a one-time bulk operation across millions of historical ledgers, the same overhead becomes the bottleneck. A local machine writing directly to a local database processes historical data at a rate that the serverless pipeline cannot match.

Cost follows the same logic. Lambda invocations, S3 data transfer, and remote database writes during a multi-week bulk ingest all accumulate. Running on hardware you already own costs nothing beyond electricity.

The lesson is straightforward: the event-driven architecture is the right shape for what it was designed for — continuous, low-latency, per-ledger ingestion. It is not the right shape for a one-time bulk operation. Recognising that distinction and choosing differently for the backfill is not a compromise on the architecture. It is the architecture applied correctly.

---

## ClickHouse on Hetzner — Why We Left AWS for the Database

When we designed the database schema in PostgreSQL and tested it against a batch of ledgers, the extrapolated storage size for the full Stellar history came to approximately 7 TB. The same data in ClickHouse takes roughly 900 MB.

That number changed the decision.

ClickHouse is a columnar database. Instead of storing rows — all fields for a given record adjacent in storage — it stores columns, all values for a given field adjacent. For blockchain data, this is a significant advantage. Account IDs, contract addresses, operation types, ledger sequences, and status codes repeat millions of times across the dataset. Column-adjacent storage lets the compression algorithm — LZ4 or ZSTD per column — exploit that repetition far more aggressively than a row-oriented layout allows. The result is storage that is not marginally smaller but categorically smaller: a dataset you would otherwise manage in terabytes fits comfortably in gigabytes.

Beyond compression, ClickHouse's columnar layout is naturally suited to the queries a block explorer serves. Every page is some form of time-range scan: recent transactions, recent invocations of a contract, event history for a ledger sequence range. Columnar databases read only the columns a query touches, which for range scans over a subset of fields is substantially less I/O than a row store that reads entire rows.

**Why not DynamoDB?** DynamoDB is optimised for single-row lookups at very high scale. A block explorer's queries — all invocations of contract X ordered by ledger, all transactions from account Y in the last thirty days, aggregate event counts by type — are not single-row lookups. Modelling them efficiently in DynamoDB would require denormalising every read pattern at write time. With a query surface as wide as ours, that is an ongoing maintenance burden every time the frontend adds a new page.

**Why Hetzner?** ClickHouse is not available as a managed service on AWS. Running it on Hetzner gives us a dedicated server with NVMe storage at a fraction of the cost of an equivalent managed database instance. Hetzner's pricing is particularly favourable for storage-heavy analytical workloads.

This makes our architecture hybrid: compute on AWS — Lambda, ECS Fargate, API Gateway, CloudFront — and database on Hetzner. The Lambda runtime does not care which cloud the database lives in. The connection is a standard TCP connection to a ClickHouse HTTP interface, the same way any other client would connect. Keeping everything under one vendor's roof is a convenience, not a technical requirement, and in this case it was a convenience we traded for a 7 TB reduction in storage.

---

## CDK in TypeScript — Infrastructure in the Monorepo

All infrastructure is defined in AWS CDK using TypeScript, in `infra/aws-cdk` inside the same Nx monorepo as the application code.

The concrete benefit is not that TypeScript is particularly special for infrastructure definition. It is that the same language, the same toolchain, and the same compiler that validate the application also validate the infrastructure. Environment names are typed constants shared across CDK and application code. S3 bucket key formats are defined once. A misconfigured Lambda environment variable that references a non-existent secret name fails at `cdk synth` rather than at runtime.

One choice worth being explicit about: GitHub Actions authenticates to AWS using OIDC rather than long-lived credentials. The Actions workflow assumes a scoped IAM role at deploy time; no AWS keys are stored in GitHub secrets. For a public repository, this is not optional — any credential committed to a public repo is compromised immediately. OIDC eliminates the credential management problem at its source.

The CDK stack is fully reproducible. The design intention from the beginning was that anyone — including the Stellar Foundation or any other team in the ecosystem — should be able to clone the repository and `cdk deploy` a complete working copy of the system in a fresh AWS account. The architecture is public infrastructure, not proprietary tooling.

---

## What We Would Do the Same Way Again

The Soroban block explorer is past Milestone 1 — historical backfill complete, live ingestion running, indexing the Stellar mainnet in real time. Looking back at the architectural choices, a few stand out as clearly right.

**Owning the live pipeline.** The decision to ingest directly from the canonical ledger — rather than routing the live path through an external chain API — has paid for itself many times over. We control what data we store, how we store it, and how we serve it. There is no upstream reliability dependency on the critical path, no rate limit, no undocumented behaviour we cannot inspect.

**Rust for Lambdas.** The cold start performance is real, the XDR parsing throughput is real, and the AI development loop has been a genuine productivity multiplier. We would make the same choice again and we expect to make it on future projects.

**Nx for the hybrid monorepo.** Coupling between frontend, backend, infrastructure, and shared libraries is not something you can wish away by splitting into multiple repositories. Making it explicit, making it visible, and tooling around it has made the codebase easier to navigate and the CI pipeline faster to run.

**ClickHouse on Hetzner.** Seven terabytes versus nine hundred megabytes is not a marginal improvement. It is a different order of infrastructure, and once we saw that number we did not seriously consider the alternative.

**The pragmatic backfill.** Running history on local machines and migrating the result was the right call. The elegant solution and the correct solution are not always the same.

---

The repository is public at [github.com/rumblefishdev/soroban-block-explorer](https://github.com/rumblefishdev/soroban-block-explorer). The grant announcement with more background on the project is on [our blog](https://www.rumblefish.dev/blog/post/rumble-fish-grant-stellar-block-explorer/). If you are building something with a similar shape — event-driven ingestion, analytical storage, serverless compute — we are happy to talk through it.
