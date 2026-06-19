//! Per-ledger processing: parse all stages.
//!
//! Pure parse half — [`parse_ledger`] runs every extract stage and
//! returns the slices in [`ParseOutput`]. No I/O, no DB.
//!
//! Persist half is driven directly from the Lambda handler in
//! [`super`] via `db_clickhouse::persist::persist_ledger_clickhouse`
//! — the same one-shot wrapper the runner's `Sink::persist_ledger`
//! fallback uses (open `PartitionWriter` → write_ledger → commit per
//! ledger). For the legacy PG flow (gated behind `pg-persist`),
//! [`process_ledger`] runs the 15-step `persist::persist_ledger`.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Instant;

use stellar_xdr::curr::{LedgerCloseMeta, TransactionMeta};
use tracing::{info, warn};
use xdr_parser::types::{
    ExtractedAccountState, ExtractedAsset, ExtractedContractDeployment, ExtractedContractInterface,
    ExtractedEvent, ExtractedInvocation, ExtractedLedger, ExtractedLiquidityPool,
    ExtractedLiquidityPoolSnapshot, ExtractedLpPosition, ExtractedNft, ExtractedNftEvent,
    ExtractedOperation, ExtractedTransaction,
};

#[cfg(feature = "pg-persist")]
use aws_sdk_cloudwatch::Client as CloudWatchClient;
#[cfg(feature = "pg-persist")]
use sqlx::PgPool;

#[cfg(feature = "pg-persist")]
use super::HandlerError;
#[cfg(feature = "pg-persist")]
use super::persist;
#[cfg(feature = "pg-persist")]
use super::persist::ClassificationCache;

/// Parsed-but-not-yet-persisted output of a single `LedgerCloseMeta`.
///
/// Mirrors the slices the writers consume, packaged so a caller can
/// drive parse and persist as separate steps (task 0205 — CH backfill
/// sink calls `parse_ledger` then a CH-flavored persist).
#[derive(Debug, Clone)]
pub struct ParseOutput {
    pub ledger: ExtractedLedger,
    pub transactions: Vec<ExtractedTransaction>,
    pub operations: Vec<(String, Vec<ExtractedOperation>)>,
    pub events: Vec<(String, Vec<ExtractedEvent>)>,
    pub invocations: Vec<(String, Vec<ExtractedInvocation>)>,
    pub contract_interfaces: Vec<ExtractedContractInterface>,
    pub contract_deployments: Vec<ExtractedContractDeployment>,
    pub account_states: Vec<ExtractedAccountState>,
    pub liquidity_pools: Vec<ExtractedLiquidityPool>,
    pub pool_snapshots: Vec<ExtractedLiquidityPoolSnapshot>,
    pub assets: Vec<ExtractedAsset>,
    pub nfts: Vec<ExtractedNft>,
    pub nft_events: Vec<ExtractedNftEvent>,
    pub lp_positions: Vec<ExtractedLpPosition>,
    pub contract_name_writes: Vec<(String, String)>,
    /// On-chain Soroban token metadata (name/symbol/decimals) from
    /// instance-storage `METADATA`, for the `soroban_contract_metadata` side
    /// table (ADR 0049). SACs already excluded by the producer.
    pub contract_metadata_writes: Vec<xdr_parser::ExtractedContractMetadata>,
    /// Per-transaction operation tree JSON, collected by `extract_invocations`.
    /// Neither write path reads it today (CH writer skips it, PG flow
    /// took it as `_operation_trees`). Kept on `ParseOutput` so the
    /// downstream wiring stays live for the eventual consumer.
    #[allow(dead_code)]
    pub operation_trees: Vec<(String, serde_json::Value)>,
    /// Parse-half wall time in ms. Surfaced for diagnostic logs;
    /// backfill-runner / backfill-bench track their own timings, the
    /// CH-write Lambda no longer logs this in-band.
    #[allow(dead_code)]
    pub parse_ms: u128,
    /// Per-tx indices skipped due to `parse_error`. Surfaced for
    /// diagnostic logs by the PG flow; CH-write Lambda flag-skips
    /// them transparently via per-tx `parse_error` field on
    /// `ExtractedTransaction`.
    #[allow(dead_code)]
    pub tx_parse_errors: Vec<usize>,
    /// Task 0218 / 0220 — SAC overrides derived from the classic-asset
    /// slice (`assets`).
    pub sac_overrides: Vec<xdr_parser::SacOverride>,
}

static NETWORK_ID: OnceLock<[u8; 32]> = OnceLock::new();
/// Trimmed passphrase string behind `NETWORK_ID`. Cached alongside the
/// hash so SAC-override derivation (which takes the passphrase string,
/// not the hash) uses the SAME network source as `network_id`, instead
/// of a hard-coded mainnet constant.
static NETWORK_PASSPHRASE: OnceLock<String> = OnceLock::new();

/// Error returned by [`init_network_id`] when the
/// `STELLAR_NETWORK_PASSPHRASE` env var is missing or empty. Mirrors
/// the `MtlsError::MissingEnv` shape so the Lambda cold-start path
/// can surface both failures uniformly via `error!` + `Init Errors`
/// CW metric.
#[derive(Debug, thiserror::Error)]
#[error(
    "STELLAR_NETWORK_PASSPHRASE env not set or empty; SAC contract_id derivation cannot proceed"
)]
pub struct NetworkIdError;

/// Eager-initialise the network-id hash at cold start. Idempotent —
/// subsequent calls return the cached value. Lambda binary calls this
/// in `main()` BEFORE `lambda_runtime::run` so a missing passphrase
/// fails the Lambda init (CW `Init Errors`), not the first per-event
/// `parse_ledger`. Backfill-runner / backfill-bench bins call it from
/// their own startup before driving the parse loop.
///
/// **Whitespace handling**: `SHA256(b"   ")` is a deterministic but
/// meaningless network id that would silently derive wrong SAC
/// contract_ids; we reject whitespace-only input the same way we
/// reject an unset variable. If trimming changed the length we
/// warn-log because that suggests an env-injection bug (trailing
/// newline / leading space) the operator should fix even though we
/// recover transparently.
///
/// Tests: `NETWORK_ID` is a process-global static. Inside `cargo
/// test -p indexer` every test that lands in `parse_ledger` shares
/// it; the first call wins and pins the passphrase for the rest of
/// the process. Tests that need a specific passphrase must either
/// (a) set `STELLAR_NETWORK_PASSPHRASE` before any `parse_ledger`
/// runs or (b) call `xdr_parser::network_id(...)` directly with
/// their fixture value.
pub fn init_network_id() -> Result<&'static [u8; 32], NetworkIdError> {
    if let Some(existing) = NETWORK_ID.get() {
        return Ok(existing);
    }
    let raw = std::env::var("STELLAR_NETWORK_PASSPHRASE").map_err(|_| NetworkIdError)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(NetworkIdError);
    }
    if trimmed.len() != raw.len() {
        warn!(
            raw_len = raw.len(),
            trimmed_len = trimmed.len(),
            "STELLAR_NETWORK_PASSPHRASE had leading/trailing whitespace; \
             trimmed before hashing — fix the env injection to avoid drift"
        );
    }
    let id = xdr_parser::network_id(trimmed);
    // `set` may race with another thread that beat us; either way the
    // value is identical (same env), so we discard the race-loser
    // result and return the actually-installed one. Cache the trimmed
    // passphrase string too, so `network_passphrase()` and `network_id`
    // always describe the same network.
    let _ = NETWORK_PASSPHRASE.set(trimmed.to_string());
    let _ = NETWORK_ID.set(id);
    Ok(NETWORK_ID.get().expect("just set"))
}

/// Trimmed network passphrase string from `STELLAR_NETWORK_PASSPHRASE`
/// — the source of truth for SAC contract_id derivation. Mirrors
/// [`network_id`]'s lazy fallback so non-Lambda callers (tests, dev
/// tools) that skipped `init_network_id` still resolve it from the
/// same env value, trimmed identically. Production pre-inits via
/// `init_network_id`, so the lazy branch is dead in the hot path.
fn network_passphrase() -> &'static str {
    NETWORK_PASSPHRASE
        .get_or_init(|| {
            std::env::var("STELLAR_NETWORK_PASSPHRASE")
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| {
                    panic!(
                        "STELLAR_NETWORK_PASSPHRASE env not set; call \
                         `init_network_id()` from the binary's cold-start path \
                         before any `parse_ledger` invocation."
                    )
                })
        })
        .as_str()
}

/// Network identifier hash. Falls back to lazy init from
/// `STELLAR_NETWORK_PASSPHRASE` for callers that did not run
/// [`init_network_id`] in their cold-start path (legacy tests, dev
/// tools). Production Lambda always pre-inits, so this lazy branch
/// is dead code in the hot path.
fn network_id() -> &'static [u8; 32] {
    NETWORK_ID.get_or_init(|| {
        let passphrase = std::env::var("STELLAR_NETWORK_PASSPHRASE").unwrap_or_else(|_| {
            panic!(
                "STELLAR_NETWORK_PASSPHRASE env not set; call \
                 `init_network_id()` from the binary's cold-start path \
                 before any `parse_ledger` invocation."
            )
        });
        xdr_parser::network_id(&passphrase)
    })
}

/// Per-ledger output kept for the legacy PG `process_ledger` path
/// (pg-persist feature only). Lambda handler reads parser slices
/// directly off `ParseOutput` and doesn't need this wrapper. Fields
/// are write-only inside the PG flow today; kept for symmetry with the
/// pre-cutover indexer Lambda enrichment publish that consumed them.
#[cfg(feature = "pg-persist")]
#[allow(dead_code)]
pub struct ProcessLedgerOutput {
    pub extracted_assets: Vec<ExtractedAsset>,
    pub ledger_sequence: u32,
    pub had_nft_mints: bool,
}

/// Legacy PG path — parse one ledger and persist via the 15-step PG
/// transaction. Only compiled when `pg-persist` feature is enabled
/// (dev / bench tools targeting PG for comparison runs). Lambda
/// binary's per-target dead-code analysis flags it because main.rs no
/// longer references the PG path — that warning is noise; the lib
/// consumer (backfill-runner / backfill-bench) is the real caller.
#[cfg(feature = "pg-persist")]
#[allow(dead_code)]
pub async fn process_ledger(
    meta: &LedgerCloseMeta,
    pool: &PgPool,
    cw_client: Option<&CloudWatchClient>,
    classification_cache: &ClassificationCache,
) -> Result<ProcessLedgerOutput, HandlerError> {
    let parsed = parse_ledger(meta);
    let ledger_sequence = parsed.ledger.sequence;
    let parse_ms = parsed.parse_ms;
    let tx_parse_errors_len = parsed.tx_parse_errors.len();
    let tx_count = parsed.transactions.len();

    let persist_timer = Instant::now();
    persist::persist_ledger(
        pool,
        &parsed.ledger,
        &parsed.transactions,
        &parsed.operations,
        &parsed.events,
        &parsed.invocations,
        &parsed.operation_trees,
        &parsed.contract_interfaces,
        &parsed.contract_deployments,
        &parsed.account_states,
        &parsed.liquidity_pools,
        &parsed.pool_snapshots,
        &parsed.assets,
        &parsed.nfts,
        &parsed.nft_events,
        &parsed.lp_positions,
        &parsed.contract_name_writes,
        &parsed.sac_overrides,
        classification_cache,
    )
    .await?;
    let persist_ms = persist_timer.elapsed().as_millis();

    info!(
        ledger_sequence,
        tx_count,
        parse_errors = tx_parse_errors_len,
        parse_ms,
        persist_ms,
        "ledger saved to database"
    );

    if let Some(cw) = cw_client {
        legacy_publish_ledger_sequence_metric(cw, ledger_sequence).await;
    }

    let had_nft_mints = parsed.nfts.iter().any(|n| n.minted_at_ledger.is_some());
    Ok(ProcessLedgerOutput {
        extracted_assets: parsed.assets,
        ledger_sequence,
        had_nft_mints,
    })
}

/// Pure parse half — runs every extract stage and returns the slices.
/// No I/O, no DB.
pub fn parse_ledger(meta: &LedgerCloseMeta) -> ParseOutput {
    let extracted_ledger = xdr_parser::extract_ledger(meta);
    let ledger_sequence = extracted_ledger.sequence;
    let closed_at = extracted_ledger.closed_at;

    let parse_timer = Instant::now();

    info!(ledger_sequence, "parsing ledger");

    let net_id = network_id();
    let extracted_transactions =
        xdr_parser::extract_transactions(meta, ledger_sequence, closed_at, net_id);

    let envelopes = xdr_parser::envelope::extract_envelopes(meta, net_id);
    let tx_metas = collect_tx_metas(meta);
    let tx_results = xdr_parser::collect_tx_results(meta);

    let mut all_operations = Vec::new();
    let mut all_events = Vec::new();
    let mut all_invocations = Vec::new();
    let mut all_operation_trees: Vec<(String, serde_json::Value)> = Vec::new();
    let mut all_contract_interfaces = Vec::new();
    let mut all_ledger_entry_changes = Vec::new();
    let mut all_nft_events = Vec::new();
    let mut tx_parse_errors = Vec::new();

    for (tx_index, ext_tx) in extracted_transactions.iter().enumerate() {
        if ext_tx.parse_error {
            warn!(
                ledger_sequence,
                tx_index,
                tx_hash = %ext_tx.hash,
                "skipping per-tx parsing for parse_error transaction"
            );
            tx_parse_errors.push(tx_index);
            continue;
        }

        let envelope = envelopes.get(tx_index).and_then(Option::as_ref);
        let tx_meta = tx_metas.get(tx_index).copied();

        if let Some(env) = envelope {
            let inner = xdr_parser::envelope::inner_transaction(env);
            let op_results = tx_results
                .get(tx_index)
                .and_then(|r| xdr_parser::tx_op_results(r));
            let ops = xdr_parser::extract_operations(
                &inner,
                tx_meta,
                op_results,
                &ext_tx.hash,
                ledger_sequence,
                tx_index,
            );
            all_operations.push((ext_tx.hash.clone(), ops));
        }

        if let Some(tm) = tx_meta {
            let events = xdr_parser::extract_events(tm, &ext_tx.hash, ledger_sequence, closed_at);
            let nft_events = xdr_parser::detect_nft_events(&events);
            all_nft_events.extend(nft_events);
            all_events.push((ext_tx.hash.clone(), events));

            if let Some(env) = envelope {
                let inner = xdr_parser::envelope::inner_transaction(env);
                let inv_result = xdr_parser::extract_invocations(
                    &inner,
                    Some(tm),
                    &ext_tx.hash,
                    ledger_sequence,
                    closed_at,
                    &ext_tx.source_account,
                    ext_tx.successful,
                );
                all_invocations.push((ext_tx.hash.clone(), inv_result.invocations));
                if let Some(tree) = inv_result.operation_tree {
                    all_operation_trees.push((ext_tx.hash.clone(), tree));
                }
            }

            let interfaces = xdr_parser::extract_contract_interfaces(tm);
            all_contract_interfaces.extend(interfaces);

            let changes = xdr_parser::extract_ledger_entry_changes(
                tm,
                &ext_tx.hash,
                ledger_sequence,
                closed_at,
            );
            all_ledger_entry_changes.push((
                ext_tx.hash.clone(),
                ext_tx.source_account.clone(),
                changes,
            ));
        }
    }

    let mut all_contract_deployments = Vec::new();
    let mut all_account_states = Vec::new();
    let mut all_liquidity_pools = Vec::new();
    let mut all_pool_snapshots = Vec::new();
    let mut all_assets = Vec::new();
    let mut all_lp_positions = Vec::new();

    let sac_identity_by_contract: HashMap<String, xdr_parser::SacAssetIdentity> =
        extracted_transactions
            .iter()
            .enumerate()
            .filter(|(_, ext_tx)| !ext_tx.parse_error)
            .filter_map(|(tx_index, _)| envelopes.get(tx_index).and_then(Option::as_ref))
            .flat_map(|env| {
                let inner = xdr_parser::envelope::inner_transaction(env);
                xdr_parser::extract_sac_identities(&inner, net_id)
            })
            .collect();

    // Task 0255 Phase 1 — per-contract op-source override for the deployer
    // attribution path. Mirrors the SAC walker shape: walks every successful
    // envelope, collects `(contract_id, deployer_strkey)` pairs from
    // top-level `CreateContract*` ops AND nested `CreateContractHostFn`
    // auth entries. The map drives `extract_contract_deployments`'s
    // `deployer_account` fallback — present entries override the tx
    // source, absent entries inherit it (the ~88 % of mainnet deploys
    // with no per-op source override).
    let deployer_by_contract: HashMap<String, String> = extracted_transactions
        .iter()
        .enumerate()
        .filter(|(_, ext_tx)| !ext_tx.parse_error)
        .filter_map(|(tx_index, _)| envelopes.get(tx_index).and_then(Option::as_ref))
        .flat_map(|env| {
            let inner = xdr_parser::envelope::inner_transaction(env);
            let tx_source = inner.source_account();
            xdr_parser::extract_op_source_per_contract(&inner, &tx_source, net_id)
        })
        .collect();

    let mut all_contract_name_writes: Vec<(String, String)> = Vec::new();
    let mut all_contract_metadata_writes: Vec<xdr_parser::ExtractedContractMetadata> = Vec::new();
    for (_tx_hash, tx_source, changes) in &all_ledger_entry_changes {
        let deployments = xdr_parser::extract_contract_deployments(
            changes,
            tx_source,
            &sac_identity_by_contract,
            &deployer_by_contract,
        );
        // detect_assets uses WASM interfaces to classify non-SAC deployments
        // as Soroban-native assets (task 0120). Contracts deployed in this
        // ledger without a matching interface are skipped here and picked up
        // by the late-WASM bridge step (task 0120) once reclassify promotes
        // them.
        let assets = xdr_parser::detect_assets(&deployments, &all_contract_interfaces);
        all_assets.extend(assets);
        let classic_credits = xdr_parser::detect_classic_credit_assets(changes);
        all_assets.extend(classic_credits);
        all_contract_deployments.extend(deployments);

        let accounts = xdr_parser::extract_account_states(changes);
        all_account_states.extend(accounts);

        let (pools, snapshots) = xdr_parser::extract_liquidity_pools(changes);
        all_liquidity_pools.extend(pools);
        all_pool_snapshots.extend(snapshots);

        let lp_pos = xdr_parser::extract_lp_positions(changes);
        all_lp_positions.extend(lp_pos);

        let name_writes = xdr_parser::extract_contract_data_name_writes(changes);
        all_contract_name_writes.extend(name_writes);
        all_contract_metadata_writes.extend(xdr_parser::extract_contract_metadata_writes(changes));
    }

    all_assets.push(xdr_parser::native_asset_singleton());

    let all_nfts = xdr_parser::detect_nfts(&all_nft_events);

    // Derive SAC overrides from the SAME network passphrase as
    // `network_id` above (not a hard-coded mainnet constant) — on a
    // non-mainnet stack a mainnet-pinned passphrase would compute
    // wrong SAC contract_ids and corrupt SAC identity fixing.
    let sac_overrides =
        xdr_parser::derive_sac_overrides_from_assets(&all_assets, network_passphrase());

    let parse_ms = parse_timer.elapsed().as_millis();

    let nft_events = xdr_parser::extract_nft_ownership_events(&all_nft_events);

    ParseOutput {
        ledger: extracted_ledger,
        transactions: extracted_transactions,
        operations: all_operations,
        events: all_events,
        invocations: all_invocations,
        contract_interfaces: all_contract_interfaces,
        contract_deployments: all_contract_deployments,
        account_states: all_account_states,
        liquidity_pools: all_liquidity_pools,
        pool_snapshots: all_pool_snapshots,
        assets: all_assets,
        nfts: all_nfts,
        nft_events,
        lp_positions: all_lp_positions,
        contract_name_writes: all_contract_name_writes,
        contract_metadata_writes: all_contract_metadata_writes,
        operation_trees: all_operation_trees,
        parse_ms,
        tx_parse_errors,
        sac_overrides,
    }
}

#[cfg(feature = "pg-persist")]
#[allow(dead_code)]
async fn legacy_publish_ledger_sequence_metric(cw_client: &CloudWatchClient, ledger_sequence: u32) {
    use aws_sdk_cloudwatch::types::{Dimension, MetricDatum, StandardUnit};
    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "unknown".to_string());
    let datum = MetricDatum::builder()
        .metric_name("LastProcessedLedgerSequence")
        .dimensions(
            Dimension::builder()
                .name("Environment")
                .value(&env_name)
                .build(),
        )
        .value(f64::from(ledger_sequence))
        .unit(StandardUnit::None)
        .build();
    let result = cw_client
        .put_metric_data()
        .namespace("SorobanBlockExplorer/Indexer")
        .metric_data(datum)
        .send()
        .await;
    if let Err(e) = result {
        warn!(ledger_sequence, error = %e, "failed to publish LastProcessedLedgerSequence metric");
    }
}

/// Collect per-transaction TransactionMeta references from any LedgerCloseMeta variant.
fn collect_tx_metas(meta: &LedgerCloseMeta) -> Vec<&TransactionMeta> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
    }
}
