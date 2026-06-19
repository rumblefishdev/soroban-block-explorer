# S — Implementation review synthesis (5-agent battery, 2026-06-18)

> Synthesis note, 2026-06-18, karolkow (Claude). Status: mature.
> Evidence FOR the implementation + the deferred-to-0303 runtime finding.
> Companion to [S-devil-advocate-full-crypto-split.md](S-devil-advocate-full-crypto-split.md)
> (the 5,607/5,607 cryptographic proof).

A 5-agent review ran over the full diff: correctness (`/review`), simplify×2
(xdr-parser + backfill), an implementation-level devil's advocate (chain/docs/
prod-`chq` verified), and a 10-point requirements/quality checklist. **All five
converged: the code is correct, safe, senior-quality, right-sized, and
consistent with project convention.** The findings worth recording:

## Evidence FOR the implementation (independently verified)

- **Crypto-match gate is fail-closed both directions.** Non-SAC emitter →
  `derived != emitter` → rejected (the 3 known shape-colliding WASM tokens stay
  `is_sac=false`); malformed topics / non-string last topic / bad issuer StrKey
  / code >12 bytes → `None`. A real un-deployed SAC always matches. A
  whole-network mismatch (wrong `--network`) → universal `None` → writes nothing
  (fails closed, never wrong).
- **`topics.last()` is spec-correct for BOTH protocol eras, all 5 signatures** —
  verified against the primary CAPs, not the repo:
  - Post-P23 (CAP-0067): asset is the final topic for transfer/mint/burn/
    clawback/set_authorized; the `set_authorized` `authorize` bool is in `data`,
    not topics.
  - Pre-P23 (CAP-0046-06): the `admin` topic sits in the MIDDLE (topic[1]); the
    asset is still LAST. The P23 change removed the middle admin topic, never
    moved the asset off the end.
  - **The code is MORE correct than the spec text:** README/notes say
    "topic[3]", which only holds for the 4-topic shapes; `burn` is 3-topic
    (`[burn, from, asset]`, asset at index 2). `topics.last()` is the
    superset-correct choice — do NOT "fix" it back to `[3]` or burn-only orphans
    silently break.
- **Same-ledger routing is fixed in-ledger, not just eventually.** Code trace of
  `prepare_with_sac_overrides`: the event-derived SAC override is pushed to
  `out.contract_rows` (`stage.rs:566-597`), then `verdict_by_contract` is rebuilt
  from `out.contract_rows` (`stage.rs:1081-1088`) BEFORE `route_for`
  (`stage.rs:1098-1104`) reads it → a SAC's own NFT-shaped events in the SAME
  ledger route to `Drop`, not pending. Cross-ledger later events are caught by
  the writer's `prior_contract_verdicts` lookup (sees the flipped `is_sac=true`).
- **RMT `version=0` override is sound** (not a new gamble): the engine reference
  defines the equal-version tie as last-inserted-wins, and the override is always
  inserted after the historical skeleton; every consumer reads via `FINAL`. This
  is byte-identical to the already-shipped task-0220 SAC-override at
  `stage.rs:579-595`. The gated e2e (`relabel_e2e…`) asserts exactly this via
  `FINAL` after a separate-part INSERT.
- **Idempotent:** after a flip the row fails the orphan predicate (`is_sac=false`
  no longer holds) → a re-run confirms/writes nothing (asserted by the e2e).
- **No sensitive data** in any changed file (only public Stellar chain data:
  issuer G-addresses, contract C-ids, asset codes). No `crates/api/**` /
  `Cargo.toml` change → api-types codegen gate N/A.

## Findings deferred (operational → 0303) or optional

- **[deferred to 0303 — prod query plan] `fetch_orphan_events` OOMs on prod.**
  The devil reproduced the exact 3.73 GiB `MEMORY_LIMIT_EXCEEDED`: the
  `soroban_contracts FINAL JOIN soroban_events` builds the (huge) events side
  into the join hash, and `signature` is NOT in the `soroban_events` sort key
  (`ORDER BY (contract_id, ledger_sequence, …)`) so it can't prune. **Even
  `--dry-run` runs this query.** Fix before the prod RUN (0303): anchor on the
  ~5,607-row orphan side — materialize the orphan `id`s, then probe
  `soroban_events WHERE contract_id IN (…)` (hits the primary key), reading
  `topics_xdr` for only those contracts. The unit logic + e2e are unaffected
  (small seed); this is purely a prod query-plan tuning, owned by the 0303 run.
- **[optional hardening] `version=1` sentinel** instead of `0` would convert the
  equal-version tie into a strict `>` (still loses to real deploys), removing the
  residual "a future stray v0 insert could un-flip" fragility — but it must be
  applied uniformly to the live 0220 `stage.rs` path AND the 0294 paths, so it
  expands scope onto shipped code. Sound without it; left as a decision.
- **[follow-up nit] `topic_symbol_value` is now duplicated 3× across `nft.rs`,
  `sac.rs`, and `event_filters.rs`** — candidate for a shared `pub(crate)`
  topic-helper module. Two of the three copies predate 0294, so this is a
  separate cleanup, not a 0294 blocker.
- **[nit] per-tx `network_id` recompute** in `process.rs` (one SHA256/tx);
  negligible, optional to hoist.

## Verdict

Senior-quality, safe, on-scope. The labeling logic is correct and the gate is
airtight; the only must-fix is the prod query plan, which belongs to the 0303
RUN (validatable only against prod CH). Architecture docs updated per ADR 0032
(event-derived SAC labeling). Step 3 (registry de-pollution) spun to 0307.
