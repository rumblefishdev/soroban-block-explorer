# Article Research Notes & Questions

## What I've Read

### Repository Architecture (from docs/)

- Full technical design in `docs/architecture/technical-design-general-overview.md`
- Separate overviews for: backend, frontend, infrastructure, indexing pipeline, XDR parsing, database schema
- Architecture is fully designed but repository is still in early implementation (skeletal code, mainly docs and lore backlog)

### Blog Context (from rumblefish.dev/blog/category/aws/)

Existing articles are conceptual/introductory:

- Serverless vs Server Computing (~1,200 words, no code, no diagrams)
- 8 Serverless Real-World Use Cases (~1,800 words, case studies, metrics)
- What is AWS Serverless Development
- Apache Flink/Kinesis for big data
- CloudFormation vs Terraform
- AWS Step Functions for Big Data

**Gap this article fills:** None of the existing articles show a _real, full-stack, production system design_. They explain concepts. This article argues from architecture decisions. It's the most technical piece on the blog and that's appropriate — it's backed by a real project.

---

## Key Architecture Summary (for drafting)

```
Stellar Network peers
  → Galexie (ECS Fargate, continuous) [writes ~1 file/5-6s]
  → S3: stellar-ledger-data/ (LedgerCloseMeta XDR, zstd-compressed)
  → Lambda: Ledger Processor (S3 PutObject trigger) [parses XDR → PostgreSQL]
  → RDS PostgreSQL (block explorer's own schema)
  → Lambda: NestJS API (API Gateway, per-request)
  → CloudFront → React SPA

Lambda: Event Interpreter (EventBridge, every 5 min)
  → reads soroban_events → writes event_interpretations (human-readable summaries)
```

**Nx monorepo packages:**

- `apps/api` — NestJS REST API (runs on Lambda)
- `apps/indexer` — Ledger Processor Lambda
- `apps/workers` — Event Interpreter Lambda
- `apps/web` — React SPA
- `infra/aws-cdk` — CDK TypeScript infrastructure
- `libs/domain` — shared explorer types, entities, identifiers
- `libs/shared` — generic utilities
- `libs/ui` — reusable React components

**Where Rust comes in:**

- Soroban smart contracts are written in Rust → compiled to WASM
- WASM binary has a metadata section with exported function signatures
- Explorer extracts interface at deploy time → `soroban_contracts.metadata`
- Soroban uses `ScVal` (XDR union type encoding Rust values) for all args/returns/events
- `ScVal` recursive decoding is a significant part of the Ledger Processor parsing work
- Invocation trees (contract-to-contract calls) stored as JSONB in `transactions.operation_tree`

**Database (correction vs. design docs):**

- NOT RDS PostgreSQL — replaced with ClickHouse running on Hetzner
- Reason: ClickHouse stores data dramatically smaller than PostgreSQL (columnar + LZ4/ZSTD compression)
- Actual measured numbers: PostgreSQL schema extrapolated to ~7 TB for full history; ClickHouse actual = ~900 MB
- Architecture is hybrid: AWS (Lambda, ECS, S3, API Gateway, CloudFront) + Hetzner (ClickHouse)
- Cost numbers removed from article per Marek

**Latency target:** <10 seconds from ledger close to DB write

---

## Questions for Marek

These are things I need clarification on before writing the final draft. I've left them as a markdown checklist so you can answer directly in this file or chat.

### 1. The "Rust" angle — confirm my reading

From the docs I see Soroban contracts are Rust → WASM. The repository has a RESEARCH task `0003_RESEARCH_soroban-wasm-interface-extraction`. My outline treats "using Rust" as _Soroban contracts being Rust programs_, not as Rust being used in the explorer itself (the explorer is TypeScript/Node). Is that the angle you meant, or did you mean something else?

- [ ] Yes, Rust = Soroban contracts are Rust → WASM, and WASM introspection is the interesting bit
- [x] No, there is actual Rust tooling in the explorer stack I'm not seeing — clarify: We're not doing really anything in particular with Soroban smart contracts. We choose Rust because it's amazingly fast and AWS has great support for this. Please notice, that even though the monorepo uses nx, the backend components are Rust crates. Historically in Rumble Fish we've prefered Typescript for backend (and still running these like lambdas). We know prefer Rust, because with advent of AI it became much more simple to write that it was. And there is tremendous upside in running spead, shorter cold start (because of bundle size). Also the Rust compiler makes it easy for AI - a lot of errors are caught on compile time, so AI agents have a very tight loop of interaction.

### 2. Project status — built or designed?

The docs say the repo is "still skeletal" and implementation hasn't started yet (it's in design/planning phase). The article outline I wrote treats this as a _design_ article ("here is the architecture we designed"), not a "here is the system running in production" article.

Is that accurate? Or is the system live / partially live?

- [ ] Correct — this is a design/architecture proposal, not a live system
- [ ] The system is live at: \***\*\_\_\_\*\***
- [x] Partially live (Deliverable 1 complete, etc.): At the moment of writing, we've almost completed the backfill of historical data, which is the Milestone 1. So this is an ongoing effort, but we're past the half mark

### 3. Audience and tone

The existing blog articles are accessible (1,200–2,000 words, no code snippets, decision-maker audience). My outline goes more technical (~2,500–3,500 words) because the content warrants it.

Are you okay with a longer, more technical piece? Or should I match the existing blog's lighter style?

- [x] Go more technical — this audience is senior devs who want depth
- [ ] Match existing style — keep it accessible to decision-makers, fewer technical details
- [ ] Something in between: \***\*\_\_\_\*\***

### 4. Rumble Fish attribution

Should the article speak in first person as Rumble Fish ("we built this for a client", "our team designed this")? Or is it framed more generically as lessons from a project?

- [x] First person Rumble Fish — "here's what we built and why"
- [ ] Client-project framing — mention it's a client project
- [ ] Generic / third-person — "here is an architecture and the reasoning behind it"

### 5. Can we name the client / project?

Is "Stellar / Soroban Block Explorer" something we can name publicly? Is there a public GitHub repo to link to?

- [x] Yes, project/client is public — GitHub: https://github.com/rumblefishdev/soroban-block-explorer — Grant announcement: https://www.rumblefish.dev/blog/post/rumble-fish-grant-stellar-block-explorer/
- [ ] No, keep it generic (just "a blockchain explorer on Stellar")
- [ ] Not yet public but will be — write as if it will be

### 6. Metrics and real numbers

The cost estimates ($425/month at 1M requests) come from the design docs — they're estimates, not actuals. Is that okay to publish, or should I soften the language ("estimated" / "projected")?

- [ ] Fine to use design-doc estimates with "estimated" qualifier
- [ ] We have actual production numbers — share: \***\*\_\_\_\*\***
- [x] Remove cost numbers entirely

### 7. The article title

Which of the working titles resonates most with you?

1. Serverless at the Edge of a Blockchain: Architecture Decisions Behind Our Soroban Explorer
2. How We Built a Production Blockchain Explorer Without Managing Servers
3. From Ledger Close to REST API in Under 10 Seconds: Our Soroban Block Explorer Architecture
4. One Monorepo, Five AWS Services, Zero Third-Party APIs: Our Soroban Block Explorer

- [x] Prefer option: Serverless at the Edge of a Blockchain: Architecture Decisions Behind Our Soroban Explorer
- [ ] Different direction: \***\*\_\_\_\*\***

### 8. Diagram inclusion

The article would benefit from at least one architecture diagram (the ingestion pipeline flow). The blog doesn't seem to use diagrams currently (based on analyzed articles).

- [x] Include a diagram — I'll provide or approve one
- [ ] Text-only is fine

### 9. Anything I missed?

Is there anything about the project's technical decisions that you feel strongly about including that isn't in my outline?

- The NestJS Lambda adapter (libraries? approach?)
- Drizzle ORM specifically (vs Prisma, TypeORM)?
- GitHub Actions + OIDC authentication approach?
- The historical backfill approach (reusing the same ECS task format + same Lambda)?
- Other: \***\*\_\_\_\*\***

---

## Blog Articles Fetched for Reference

Saved for style analysis:

- `https://www.rumblefish.dev/blog/post/serverless-vs-server-computing-detailed-comparison/`
  - ~1,200 words, professional/accessible, no diagrams, no code
- `https://www.rumblefish.dev/blog/post/what-are-serverless-examples-8-real-world-use-cases-of-serverless-technology/`
  - ~1,800 words, case studies + metrics, no code

---

## Drafting Notes

Once questions above are answered, write the draft in `draft.md`. Structure:

1. Fill in actual project status language (designed vs live)
2. Confirm Rust angle
3. Add any diagram assets to `article/diagrams/`
4. Match tone to Marek's preference on audience

The sections with strongest differentiation vs. existing Rumble Fish blog content:

- **Section 3 (ingestion pipeline)** — the ECS vs Lambda split, S3 as durable handoff
- **Section 4 (Rust/WASM)** — unique to this project, nothing like it on the blog
- **Section 8 (costs)** — concrete numbers, rare in blog posts
