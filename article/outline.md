l# Article Outline (v2 — post-answers)

**Title:** Serverless at the Edge of a Blockchain: Architecture Decisions Behind Our Soroban Explorer

**Subtitle candidate:** How we combined Nx, Rust Lambdas, and event-driven AWS to index the Stellar network in real time

**Target length:** ~2,500–3,500 words (senior dev audience, more technical than typical Rumble Fish posts)

**Voice:** First-person Rumble Fish ("we", "our team")

**Status framing:** Project is live and past Milestone 1 — historical backfill is nearly complete, live ingestion is running

**Key decisions to argue:**

1. Own the data pipeline — no third-party indexer dependency
2. Nx monorepo for a hybrid TypeScript + Rust workspace
3. ECS Fargate for continuous processes, Lambda for event-driven work — don't apply one model to everything
4. Rust for all backend Lambdas — speed, cold start, AI-agent development loop
5. S3 as a durable handoff + replay surface between ingestion stages
6. PostgreSQL over NoSQL for relational explorer data

**NOT included:** cost estimates (removed per Marek's answer)

---

## Intro (~250 words)

**Hook:** Start with the grant announcement as context — Rumble Fish received a $131,200 Stellar Community Fund grant to build a Soroban-first block explorer. Set up the challenge immediately: real-time blockchain indexing with no dependency on third-party APIs, publicly funded, open source from day one.

Frame the tension: block explorers sound simple on the surface — show transactions, show accounts — but the Stellar / Soroban protocol is anything but simple. Each ledger close produces a binary XDR payload containing every account change, transaction result, smart contract invocation, and emitted event. You need to ingest it in near-real time, decode it, store it, and serve it — all while the next ledger closes 5 seconds later.

This article walks through the architecture decisions that shaped our solution. Some of them are unusual. Specifically, building the backend on Rust in a monorepo that also contains TypeScript and React, for reasons that go beyond performance.

---

## Section 1: Why We Don't Call Any External API (~300 words)

**Core argument: own your data or own your outages**

The quick-and-dirty approach is wrapping Horizon or the Soroban RPC. We rejected it for three reasons:

1. **Horizon is deprecated** for the indexing use cases we need. The Stellar Foundation's own tooling has moved beyond it.
2. **Rate limits and SLA dependency.** A block explorer that depends on someone else's explorer inherits their reliability and their limitations. During network load spikes — exactly when your tool matters most — rate limits kick in.
3. **Soroban data isn't fully surfaced by existing indexers.** We need decoded contract invocation trees, human-readable ScVal arguments, WASM-derived function signatures, and CAP-67 events. None of this is available pre-assembled from existing APIs.

**Decision:** ingest directly from the canonical ledger using Galexie and Captive Core. Own the database. Serve from our schema.

This one decision shapes everything: our ingestion pipeline, our database ownership, our parsing layer, and our infrastructure model. If we had accepted a third-party API dependency, most of the rest of this article wouldn't exist.

---

## Section 2: One Monorepo, Multiple Languages — The Nx Choice (~400 words)

**Core argument: the monorepo isn't about simplicity, it's about managing coupling across a complex system**

The project has eight distinct packages, spread across TypeScript and Rust:

| Package         | Language   | Role                                    |
| --------------- | ---------- | --------------------------------------- |
| `apps/indexer`  | Rust       | Ledger Processor Lambda — XDR ingestion |
| `apps/workers`  | Rust       | Event Interpreter Lambda — enrichment   |
| `apps/api`      | Rust       | REST API Lambda                         |
| `apps/web`      | TypeScript | React SPA                               |
| `infra/aws-cdk` | TypeScript | CDK infrastructure                      |
| `libs/domain`   | TypeScript | Shared explorer types                   |
| `libs/shared`   | TypeScript | Generic utilities                       |
| `libs/ui`       | TypeScript | Reusable React components               |

These are not independent microservices. They share concepts — the domain types used by the frontend are the same concepts the indexer writes to the database. The CDK stack deploys all three Lambda functions. A change to the database schema touches both the indexer and the API.

**Why Nx for a hybrid workspace:**

- Affected-only builds: `nx affected --target=build` runs builds only for packages touched by a change. With eight packages, this is not a nice-to-have.
- Task orchestration: `nx run-many -t typecheck` across all TypeScript packages, `nx run-many -t build` across all packages including Rust, in one command.
- Dependency graph visualization: understanding which package depends on what is crucial when the workspace spans two languages and multiple execution environments.
- Consistent project conventions: every package follows the same task naming, CI contract, and configuration structure regardless of whether it's TypeScript or Rust under the hood.

**The alternative would have been multiple repositories.** We rejected it because the coupling is real and explicit. Separate repos hide coupling — they don't eliminate it. With Nx, when we change the database schema, we immediately see which downstream packages need to be rebuilt and retested. That visibility is the whole point.

Include diagram: Nx project dependency graph — shows `api` and `indexer` depending on `domain`, `web` depending on `domain` and `ui`, `aws-cdk` as an isolated infra node.

---

## Section 3: Why We Write Our Lambdas in Rust (~500 words)

**Core argument: Rust + AWS Lambda is an unusually good combination, and AI makes Rust accessible enough to justify the tradeoff**

Historically, Rumble Fish has written backends in TypeScript. It's ergonomic, the ecosystem is rich, and the learning curve is manageable. For this project we made a different choice — all three Lambda-based backend components (Ledger Processor, Event Interpreter, REST API) are Rust crates.

Here is why.

**Cold start performance**

Lambda cold starts are determined by two things: bundle size and initialization cost. A TypeScript Lambda typically bundles hundreds of kilobytes to a few megabytes of Node.js code plus dependencies. A compiled Rust Lambda binary is typically under 5 MB, statically linked, with no runtime to warm up. The result is cold starts measured in milliseconds rather than seconds — and on ARM/Graviton2, Rust Lambda performs particularly well because the AArch64 toolchain is mature and the binaries are compact.

For a blockchain explorer Lambda handling ~17,000 invocations per day (one per ledger close for the Ledger Processor), the cold start delta is real. Provisioned concurrency can mitigate it for the API, but the Ledger Processor fires on every S3 event — provisioned concurrency doesn't help per-event functions in the same way.

**Execution speed**

XDR parsing is not a trivial operation. Each `LedgerCloseMeta` payload contains every transaction, operation, Soroban invocation, CAP-67 event, and ledger entry change for a full ledger close. That's potentially thousands of decode operations — binary deserialization, tree traversal, JSONB construction — per invocation. Rust handles this roughly an order of magnitude faster than equivalent Node.js, with predictable memory usage and no garbage collection pauses.

Our target is under 10 seconds from ledger close to database write. With the Ledger Processor in Rust and ClickHouse on the receiving end, we're comfortably inside that budget.

**The AI development loop**

This is the argument we didn't expect to be making two years ago: Rust is now a good choice for teams working with AI-assisted development.

The Rust compiler is famously strict. Every ownership violation, every type mismatch, every lifetime error surfaces at compile time with a precise message. This turns out to be exceptionally valuable when you're working with an AI agent — the agent writes code, the compiler rejects it with a specific error, the agent reads the error and corrects it. The feedback loop is fast and deterministic. There's no "it ran but produced the wrong result" class of error that requires runtime debugging.

With TypeScript or Python, AI-generated code may be syntactically valid and dynamically typed in ways that fail silently at runtime. With Rust, errors are loud, early, and specific. The compiler effectively acts as a tight integration test on every save.

As AI assistance has made Rust more accessible — removing much of the manual borrow-checker wrestling that historically made it a steep climb — the combination of performance and compile-time correctness becomes a genuine competitive advantage, not just a talking point.

**AWS has first-class Rust support**

The `lambda_runtime` crate, the `aws-sdk-rust` collection, and the `cargo-lambda` toolchain make deploying Rust Lambdas to AWS a straightforward process. There's no fighting the platform; Rust is a supported runtime with good tooling.

---

## Section 4: The Ingestion Pipeline — Matching Compute to Workload Shape (~450 words)

**Core argument: not every problem is a Lambda problem; use the right execution model per job**

The ingestion pipeline has three distinct compute workloads and we use three different execution models.

### Galexie on ECS Fargate — for continuous, stateful work

Galexie is the Stellar Foundation's tool for streaming raw ledger data. It maintains a persistent connection to Stellar network peers via Captive Core, listens to the ledger-close stream in real time, and writes one `LedgerCloseMeta` XDR file to S3 roughly every 5–6 seconds.

Lambda is the wrong shape for this. Galexie is a long-running process that:

- Maintains a persistent peer connection
- Tracks checkpoint state between ledger closes
- Resumes from the last exported ledger on restart

Lambda has a 15-minute maximum execution time and no concept of "resume where I left off." ECS Fargate gives us a managed container runtime — one task, always running, no EC2 to manage.

### Lambda (Rust) for the Ledger Processor — for event-driven, bounded work

When Galexie writes a file to S3, an S3 PutObject notification triggers our Ledger Processor Lambda. This is textbook Lambda: one event, one bounded unit of work, invoked ~17,000 times per day, completing in under 10 seconds.

The Lambda:

1. Downloads and decompresses the zstd-compressed XDR
2. Parses every transaction, operation, invocation, and event using Stellar's XDR schema
3. Writes all structured records to PostgreSQL in a single database transaction

If the Lambda fails, Lambda retries automatically. If there's a permanent failure, the file stays in S3 — we can replay any ledger by re-triggering the Lambda with its S3 key. This replayability is why S3 is the handoff between Galexie and the Ledger Processor rather than a direct function call or a queue.

### Lambda (Rust) for the Event Interpreter — for periodic enrichment

A third Lambda fires every 5 minutes via EventBridge. It reads recently stored Soroban events and pattern-matches against known DeFi protocols to generate human-readable summaries: "Swapped 100 USDC for 95.2 XLM on Soroswap."

This is separated from the Ledger Processor deliberately. Interpretation heuristics evolve. As we recognize new protocols and improve our pattern matching, we want to update the Event Interpreter without touching the ingestion path — and re-run interpretation over historical events when our patterns improve.

Include diagram: end-to-end ingestion flow:

```
Stellar peers → Galexie (ECS Fargate) → S3 → Ledger Processor Lambda → ClickHouse (Hetzner)
                                                                              ↑
                                              Event Interpreter Lambda ───────┘
                                              (EventBridge, every 5 min)

API Gateway → API Lambda (Rust) → ClickHouse (Hetzner)
CloudFront → React SPA
```

---

## Section 5: The Historical Backfill — When Architecture Meets Reality (~250 words)

**Core argument: the elegant solution and the pragmatic solution are not always the same; know when to choose speed over purity**

Soroban launched on Stellar mainnet in late 2023. Our explorer needs to index everything from that point forward. The live Galexie process covers new ledgers, but someone has to fill in the history.

The design called for a separate ECS Fargate task to replay history through the same S3 → Lambda → ClickHouse pipeline as live ingestion — one code path for both problems.

We did not do that.

Instead we ran the backfill on local machines, writing directly to a local ClickHouse instance. Once complete, we migrated that data to production ClickHouse on Hetzner.

Two reasons: speed and cost.

**Speed:** Local machines don't have Lambda's per-invocation overhead, cold starts, or the round-trip of writing to S3 and waiting for an event trigger. Processing years of historical ledgers in bulk on a local machine with a local database is orders of magnitude faster than funneling the same work through a serverless event-driven pipeline. When you need to index Soroban's entire mainnet history before launch, that difference is the difference between days and weeks.

**Cost:** Lambda invocations, S3 transfers, and Hetzner compute during a multi-week bulk ingest all cost money. Running the backfill on hardware you already own costs nothing except electricity.

The lesson: the event-driven pipeline is exactly the right shape for live, continuous ingestion — one ledger at a time, indefinitely. It is not the right shape for a one-time bulk operation across millions of historical records. Recognising that distinction and using a different approach for each is not a failure to follow the architecture — it is the architecture being applied correctly.

---

## Section 6: ClickHouse on Hetzner — Why We Left AWS for the Database (~400 words)

**Core argument: cloud-native doesn't mean all-AWS; use the right database for the workload, wherever it runs**

The database is ClickHouse, running on Hetzner — not an AWS-managed service. This is deliberately off-AWS.

**Why ClickHouse?**

A block explorer is fundamentally an analytical read workload. Every page involves a time-range scan over a large dataset: recent transactions, recent invocations of a contract, event history for a ledger sequence range. These are exactly the queries a columnar database is built for.

The result that convinced us: when we tested our PostgreSQL schema on a batch of ledgers and extrapolated to the full Stellar history, we were looking at roughly 7 TB of storage. Our ClickHouse instance holds the same data in approximately 900 MB.

That is not a rounding error. Blockchain data is highly repetitive — account IDs, contract addresses, operation types, and status codes appear millions of times across the dataset. ClickHouse's columnar layout stores each column separately and applies per-column compression (LZ4/ZSTD), which exploits that repetition far more aggressively than a row-oriented database can. The numbers reflect that difference at scale.

Smaller storage means faster scans, lower I/O, and a meaningfully smaller infrastructure footprint. For a system that ingests a new ledger every 5–6 seconds indefinitely, a storage profile measured in megabytes rather than terabytes is a fundamentally different operational reality.

**Why not DynamoDB?**

DynamoDB is optimized for single-row lookups at scale. A block explorer's query patterns — "all transactions from account X in the last 30 days," "all invocations of contract Y ordered by ledger," "aggregate event counts by type" — are not single-row lookups. Modeling them efficiently in DynamoDB requires denormalizing every pattern at write time. With a query surface as wide as ours, that becomes an ongoing maintenance burden every time the frontend adds a new view.

**Why Hetzner?**

ClickHouse is not available as a managed service on AWS (ClickHouse Cloud exists but is its own infrastructure). Running it on Hetzner gives us a dedicated server with NVMe storage at a fraction of the cost of an equivalent RDS instance. Hetzner's European data center pricing is particularly favorable for storage-heavy analytical workloads.

This makes our architecture deliberately hybrid: compute (Lambda, ECS Fargate, API Gateway, CloudFront) on AWS, database on Hetzner. The Lambdas connect to ClickHouse over the network the same way they would connect to RDS — nothing in the Lambda runtime cares which cloud the database lives in.

The pragmatic lesson: "serverless on AWS" doesn't mean every component has to be an AWS service. The right database for the workload matters more than keeping everything under one cloud vendor's roof.

---

## Section 7: CDK in TypeScript — Infrastructure in the Monorepo (~200 words)

**Core argument: IaC in the same language as the rest of your tooling closes the feedback loop**

All infrastructure is AWS CDK in TypeScript, living in `infra/aws-cdk` inside the same Nx monorepo.

The benefit isn't that TypeScript is special for infrastructure — it's that the same types, the same toolchain, and the same CI pipeline that govern the application also govern the infrastructure. Environment names are constants. S3 bucket key formats are shared. The same `nx affected` run that determines which Lambdas need rebuilding can inform which CDK stacks need redeploying.

One specific choice worth naming: GitHub Actions uses OIDC to authenticate to AWS. No long-lived AWS credentials stored in GitHub secrets. The Actions workflow assumes a scoped IAM role at deploy time. For a public repository — which this will be — this is not optional. Any credential committed to a public repo is immediately at risk.

The full CDK stack is reproducible. The intent from the start was that anyone — including the Stellar Foundation or any other team in the ecosystem — can fork the repository and `cdk deploy` a complete working copy of the system in a fresh AWS account.

---

## Conclusion (~200 words)

**What this stack is actually about:**

A serverless blockchain explorer sounds like it should be straightforward. The reality is that the Stellar protocol produces a complex binary payload every 5 seconds, containing data that no existing API pre-assembles the way we need. Everything in our architecture is a response to that constraint.

Specific things we'd do the same way again:

- **Rust for Lambdas.** The performance is real, the cold start advantage is real, and the AI development loop is an underrated productivity multiplier.
- **Nx for the hybrid monorepo.** Coupling between frontend, backend, and infrastructure is not something you can wish away. Making it explicit and tooling around it is better than pretending it doesn't exist.
- **ECS for Galexie, Lambda for everything else.** Matching execution model to workload shape is more important than picking one model and applying it everywhere.
- **S3 as the handoff.** The replay capability has already saved us time during development. It'll save us more in production.
- **ClickHouse on Hetzner.** An order of magnitude smaller storage than PostgreSQL, faster analytical scans, and no allegiance to one cloud vendor's pricing.

Link back to: the SDF grant announcement (https://www.rumblefish.dev/blog/post/rumble-fish-grant-stellar-block-explorer/), the public GitHub repo (https://github.com/rumblefishdev/soroban-block-explorer).

CTA: building something similar or have questions about the architecture? Reach out to Rumble Fish.

---

## Style Notes

- Match the 2018 Infura article: first-person "we", problem-first, justified decisions, code/config snippets where helpful
- Include at least one architecture diagram (ingestion flow is the most important)
- Short paragraphs, numbered lists for multi-point arguments
- No cost numbers (removed per Marek)
- Write as "nearly live" — past Milestone 1, historical backfill running
