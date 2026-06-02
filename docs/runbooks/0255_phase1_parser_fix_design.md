# Task 0255 Phase 1 — Parser deployer_id fix design

**Status:** implemented — landed in PR
[#213](https://github.com/rumblefishdev/soroban-block-explorer/pull/213).
Retained as a design record for the reasoning that led to Design A and
the test-fixture taxonomy; the runtime contract is the code +
canonical docs (`docs/architecture/xdr-parsing/xdr-parsing-overview.md`
and `docs/architecture/database-schema/database-schema-overview.md`).

## Problem

`crates/xdr-parser/src/state.rs:91`:

```rust
deployments.push(ExtractedContractDeployment {
    contract_id,
    wasm_hash,
    deployer_account: Some(tx_source_account.to_string()),  // BUG
    ...
});
```

`tx_source_account` is the **inner-tx source** (per the fee-bump unwrap
already done at the call site in `crates/indexer/src/handler/process.rs`).
The Stellar protocol allows individual operations to override the tx
source via `op.source_account`. For Soroban CreateContract paths the
correct deployer is the **op-level source**, falling back to tx source
only when the op inherits.

Empirical evidence (task 0252 Phase B E11 + scale probe on Hetzner CH):

- 2,825 / 23,730 contracts (~12 %) have a per-op override and are
  therefore mis-attributed by the current parser.
- The remaining ~88 % accidentally land correctly because the op
  inherits the tx source (no override), and `tx_source ==
effective_source` in that case.

## Where the op source lives in XDR

Per Stellar XDR (`Operation` struct):

```text
Operation {
    sourceAccount: MuxedAccount?,   // None => inherits tx source
    body: OperationBody              // InvokeHostFunction op for Soroban
}
```

For Soroban-native CreateContract (factory pattern), the deploy may
be expressed in one of three shapes:

| Shape                                                                           | Where the deployer signs                                    | Parser observation surface          |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------- | ----------------------------------- |
| Top-level `InvokeHostFunction(CreateContract)` op                               | `op.sourceAccount` (per-op override) or tx source (inherit) | `Operation` envelope                |
| Top-level InvokeContract op that internally calls CreateContract via auth entry | `SorobanAuthorizationEntry.credentials.sourceAccount`       | `op.body.invokeHostFunction.auth[]` |
| Factory sub-deploy with sub-call's own source (rare)                            | Same auth-entry signer                                      | Same                                |

Our current parser already walks auth entries for SAC identity
extraction (`crate::sac::extract_sac_identities`). We extend that
pattern to extract op-level deployer per contract_id for non-SAC
deploys too.

## Threading data from caller into `extract_contract_deployments`

Call site `crates/indexer/src/handler/process.rs:322`:

```rust
let deployments =
    xdr_parser::extract_contract_deployments(changes, tx_source, &sac_identity_by_contract);
```

`changes` is the flattened `ExtractedLedgerEntryChange` list — entry-level
state, NOT op envelope. We need access to the op envelope per-tx to
read `op.source_account` + `auth[].credentials.sourceAccount`.

### Design A — extend signature with `deployer_by_contract` override map

Build the map in `process.rs` analogous to `sac_identity_by_contract`:

```rust
let deployer_by_contract: HashMap<String, String> =
    extracted_transactions
        .iter()
        .enumerate()
        .filter(|(_, ext_tx)| !ext_tx.parse_error)
        .filter_map(|(tx_index, _)| envelopes.get(tx_index).and_then(Option::as_ref))
        .flat_map(|env| {
            let inner = xdr_parser::envelope::inner_transaction(env);
            xdr_parser::extract_op_source_per_contract(&inner, tx_source_of(env))
        })
        .collect();
```

New helper `extract_op_source_per_contract(inner_tx, tx_source) ->
Vec<(contract_id, strkey)>`:

1. Walk `inner_tx.operations`.
2. For each `Operation` with `body == InvokeHostFunction`:
   - Determine `effective_source = op.sourceAccount.or(tx_source)`.
   - If `host_function == CreateContract`: derive `contract_id` from
     the `ContractIdPreimage` (reuse `crate::sac::derive_sac_contract_id`
     logic; for non-SAC use `derive_create_contract_id` —
     check if already exists or add).
     Push `(contract_id, effective_source)`.
   - If `host_function == InvokeContract`: walk `op.body.invokeHostFunction.auth[]`: - For each `SorobanAuthorizationEntry` with `credentials ==
SourceAccount(sa)`, push `(contract_id_from_auth, strkey_of(sa))`. - For `credentials == Address(addr)` (account-typed): push
     `(contract_id_from_auth, strkey_of(addr))`. - Skip when credentials is a contract address (sub-contract call —
     no human "deployer" there).

Then `extract_contract_deployments` consumes the map:

```rust
pub fn extract_contract_deployments(
    changes: &[ExtractedLedgerEntryChange],
    tx_source_account: &str,
    sac_identities: &HashMap<String, SacAssetIdentity>,
    deployer_by_contract: &HashMap<String, String>,   // NEW
) -> Vec<ExtractedContractDeployment> {
    // ...existing extraction loop...

    deployments.push(ExtractedContractDeployment {
        contract_id: contract_id.clone(),
        wasm_hash,
        deployer_account: deployer_by_contract
            .get(&contract_id)
            .cloned()
            .or_else(|| Some(tx_source_account.to_string())),
        // ...
    });
}
```

**Pros**: surgical, no refactor of `extract_contract_deployments`
internal data flow; map is empty for trivial single-source single-op
txs (the 88 % case), so behaviour stays identical there.
**Cons**: caller must build the map upfront — minor extra walk over
envelopes, but same shape as the SAC walker already present.

### Design B — refactor `extract_contract_deployments` to walk ops directly

Pass envelopes instead of `changes`, walk ops in the function, use
ledger entry changes only for the wasm_hash extraction. Bigger
refactor; cross-impact on `extract_account_states`,
`extract_liquidity_pools`, `detect_assets` callers all in the same
loop. **Reject** — too invasive for a focused bug fix.

**Decision: Design A.**

## Implementation steps

1. **Add helper** `crates/xdr-parser/src/lib.rs` →
   `pub fn extract_op_source_per_contract(tx: &Transaction, tx_source: &str)
-> Vec<(String, String)>`.
   New file `crates/xdr-parser/src/op_source.rs` or extend
   `crates/xdr-parser/src/sac.rs` if logically close.

2. **Thread map** into `extract_contract_deployments` (Design A).
   Update the single call site at `process.rs:322`. All 5 existing
   test fixture call sites in `state.rs:1181-` pass an empty
   `HashMap::new()` to preserve current behaviour.

3. **Test fixtures** in `crates/xdr-parser/tests/fixtures/0255_*.xdr`:

   - `0255_single_source_deploy.xdr` — plain InvokeHostFunction
     CreateContract with `op.sourceAccount = None`. Expected:
     `deployer_account = tx_source`.
   - `0255_per_op_override_deploy.xdr` — tx source = A, op source = B
     (explicit). Expected: `deployer_account = B`.
   - `0255_fee_bump_deploy.xdr` — feeBump.feeSource = C, inner tx
     source = A, op source = B. Expected: `deployer_account = B`
     (never A, never C).

   Each fixture: hand-built XDR via the `stellar-xdr` crate or
   captured from a real mainnet tx (preferred — copy CB5GADAT... tx
   `029fe1ca5d9c6b8d5354ece52cb29c5471c431e42a573c56a1d508a06bd87a16`
   for the third fixture).

4. **Tests in `crates/xdr-parser/tests/contract_deployer.rs`** (new):

   ```rust
   #[test]
   fn single_source_deploy_uses_tx_source() { ... }

   #[test]
   fn per_op_override_uses_op_source() { ... }

   #[test]
   fn fee_bump_uses_op_source_not_fee_source() { ... }
   ```

5. **`cargo nextest run -p xdr-parser`** must remain green.

6. **`nx run rust:lint`** + clippy `-D warnings` clean.

## Risk + roll-out

- **Risk:** the helper might mis-attribute when auth entries reference
  contract addresses (not accounts). Guard via type check on
  `SorobanCredentials` variant. Document the skip path with a tracing
  log so unrecognised shapes are observable in production.
- **Roll-out:** parser fix lands in indexer image. Live-mode ingestion
  (post task 0241 cutover) consumes the new logic. Existing CH
  backfill state is already corrected by Phase 2 migration. No
  re-backfill required.

## Acceptance for Phase 1 (cross-link to task 0255 AC)

- [ ] Parser fix lands on develop with three new unit tests covering
      single-source, multi-source override, and fee-bump cases.

## Out of scope

- Re-running task 0252 E11 → that's Phase 3 (separate, after Phase 1
  lands).
- Updating `docs/architecture/database-schema/canonical-tables.md` for
  deployer_id semantic — that's Phase 1 docs requirement, lands with
  the same PR.
- Per-op source for non-deployer attribution paths (e.g. NFT mint
  attribution, classic op attribution). Different bug surface; out of
  scope for 0255.
