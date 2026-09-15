//! Operation extraction from transaction envelopes.
//!
//! Extracts per-operation structured data with type-specific JSON details.
//! INVOKE_HOST_FUNCTION operations get enriched extraction: contractId,
//! functionName, functionArgs (ScVal decoded), and returnValue.

use crate::envelope::{InnerTxRef, muxed_id, muxed_to_g_strkey};
use crate::scval::scval_to_typed_json;
use crate::types::ExtractedOperation;
use domain::OperationType;
use serde_json::{Value, json};
use stellar_xdr::*;

/// Extract all operations from a transaction envelope, with optional return
/// value from the transaction meta (for INVOKE_HOST_FUNCTION).
///
/// `tx_meta` is needed to extract the Soroban return value. Pass the
/// `TransactionMeta` from the processing result.
///
/// `op_results` is needed to extract the liquidity pools crossed by
/// path-payment ops (claim atoms live in the result, not the envelope —
/// task 0261). Pass `tx_op_results(&TransactionResult)`; index-aligned
/// with the envelope's operations. `None` degrades gracefully: details
/// simply lack `poolIds` / `claimedAtoms`.
pub fn extract_operations(
    envelope: &InnerTxRef<'_>,
    tx_meta: Option<&TransactionMeta>,
    op_results: Option<&[OperationResult]>,
    transaction_hash: &str,
    ledger_sequence: u32,
    tx_index: usize,
) -> Vec<ExtractedOperation> {
    let ops = match envelope {
        InnerTxRef::V0(tx) => tx.operations.as_slice(),
        InnerTxRef::V1(tx) => tx.operations.as_slice(),
    };

    let return_value = tx_meta.and_then(soroban_return_value);
    // Resolved once; the fallback issuer for `allow_trust` ops that inherit it.
    let tx_source = envelope.source_account();

    ops.iter()
        .enumerate()
        .map(|(i, op)| {
            // operation_index is 1-based to match Stellar ecosystem convention
            // (Horizon paging_token encodes op_app_order in low 12 bits, also
            // 1-based). Surfaces as user-facing `application_order` in
            // `XdrOperationDto`. See task 0172 / ADR 0028.
            let op_index = i + 1;
            let source_account = op.source_account.as_ref().map(muxed_to_g_strkey);
            // Task 0359: every asset the op touches — body + same-op meta changes
            // (no result grain; see `asset_appearances` docs). `op_source` (op
            // override or tx source) is the AllowTrust issuer; it borrows
            // `source_account`, released before the move below.
            let op_source = source_account.as_deref().unwrap_or(&tx_source);
            let op_changes = op_meta_changes(tx_meta, i);
            let asset_appearances =
                crate::asset_appearances::emit_asset_appearances(&op.body, op_source, op_changes);
            // Shared by details (poolIds/claimedAtoms) and counterparties
            // (crossed-offer sellers); both read the same per-op result.
            let op_result = op_results.and_then(|rs| rs.get(i));
            let counterparties =
                crate::op_participants::extract_counterparties(&op.body, op_result);
            let (op_type, details) = extract_op_details(
                &op.body,
                return_value.as_ref(),
                op_result,
                op_changes,
                ledger_sequence,
                tx_index,
                op_index,
            );
            ExtractedOperation {
                transaction_hash: transaction_hash.to_string(),
                operation_index: u32::try_from(op_index)
                    .expect("operation index does not fit into u32"),
                op_type,
                source_account,
                details,
                asset_appearances,
                counterparties,
                source_muxed_id: op.source_account.as_ref().and_then(muxed_id),
                destination_muxed_id: destination_muxed_id(&op.body),
            }
        })
        .collect()
}

/// The multiplexing id of an operation's destination when it is an `M…`
/// address. Only the four classic operations whose destination is a
/// `MuxedAccount` can carry one (CAP-27); everything else → `None`. The `G…`
/// half is what `details.destination` already records.
fn destination_muxed_id(body: &OperationBody) -> Option<u64> {
    // Exhaustive on purpose: a future operation with a `MuxedAccount`
    // destination must be a compile error here, not a silently NULL column.
    match body {
        OperationBody::Payment(op) => muxed_id(&op.destination),
        OperationBody::PathPaymentStrictReceive(op) => muxed_id(&op.destination),
        OperationBody::PathPaymentStrictSend(op) => muxed_id(&op.destination),
        OperationBody::AccountMerge(destination) => muxed_id(destination),
        // `CreateAccount.destination` is an `AccountId`, never muxed.
        OperationBody::CreateAccount(_)
        | OperationBody::ManageSellOffer(_)
        | OperationBody::CreatePassiveSellOffer(_)
        | OperationBody::SetOptions(_)
        | OperationBody::ChangeTrust(_)
        | OperationBody::AllowTrust(_)
        | OperationBody::Inflation
        | OperationBody::ManageData(_)
        | OperationBody::BumpSequence(_)
        | OperationBody::ManageBuyOffer(_)
        | OperationBody::CreateClaimableBalance(_)
        | OperationBody::ClaimClaimableBalance(_)
        | OperationBody::BeginSponsoringFutureReserves(_)
        | OperationBody::EndSponsoringFutureReserves
        | OperationBody::RevokeSponsorship(_)
        | OperationBody::Clawback(_)
        | OperationBody::ClawbackClaimableBalance(_)
        | OperationBody::SetTrustLineFlags(_)
        | OperationBody::LiquidityPoolDeposit(_)
        | OperationBody::LiquidityPoolWithdraw(_)
        | OperationBody::InvokeHostFunction(_)
        | OperationBody::ExtendFootprintTtl(_)
        | OperationBody::RestoreFootprint(_) => None,
    }
}

/// Per-operation results of a **successful** transaction.
///
/// Unwraps the fee-bump nesting (`TxFeeBumpInnerSuccess` carries the inner
/// transaction's per-op results one level down). Returns `None` for any failed
/// transaction: when a transaction fails, every operation is rolled back, yet
/// the op-level result of an operation that executed before the failing one
/// still shows `Success` (with claim atoms) for crossings that never
/// committed. Gating on tx success here keeps those phantom crossings out of
/// `pool_ids` and the downstream `gross_volume_a` (task 0261). Claim atoms are
/// the sole consumer of this accessor, so success-only is the correct scope.
pub fn tx_op_results(result: &TransactionResult) -> Option<&[OperationResult]> {
    match &result.result {
        TransactionResultResult::TxSuccess(ops) => Some(ops.as_slice()),
        TransactionResultResult::TxFeeBumpInnerSuccess(pair) => match &pair.result.result {
            InnerTransactionResultResult::TxSuccess(ops) => Some(ops.as_slice()),
            _ => None,
        },
        _ => None,
    }
}

/// Per-operation results of ANY applied transaction — success or failed.
///
/// Companion to [`tx_op_results`], which stays success-only because its sole
/// consumer is claim atoms (task 0261 — rolled-back crossings must not reach
/// `poolIds`). `TxFailed` carries the same per-op array as `TxSuccess`, and
/// the failing operation's code IS the transaction's fail reason (task 0352),
/// so this accessor unwraps all four applied arms. `None` only for
/// validation-level failures (`TxBadSeq`, `TxInsufficientFee`, …), where no
/// operation was ever attempted and no per-op array exists.
pub fn tx_op_results_any(result: &TransactionResult) -> Option<&[OperationResult]> {
    use TransactionResultResult::*;
    match &result.result {
        TxSuccess(ops) | TxFailed(ops) => Some(ops.as_slice()),
        TxFeeBumpInnerSuccess(pair) | TxFeeBumpInnerFailed(pair) => match &pair.result.result {
            InnerTransactionResultResult::TxSuccess(ops)
            | InnerTransactionResultResult::TxFailed(ops) => Some(ops.as_slice()),
            _ => None,
        },
        _ => None,
    }
}

/// The result code of a single operation, named by the XDR library's own
/// variant names — no hand-rolled name table (the 0431 lesson).
///
/// `OpInner` unwraps to the operation-specific result variant (`"Success"`,
/// `"LowReserve"`, `"Trapped"`, …); the operation-level rejections keep their
/// `Op…` names (`"OpNoAccount"`, `"OpBadAuth"`, …). Task 0352: the failing
/// operation's code is what the fail-reason banner shows.
pub fn op_result_code(op_result: &OperationResult) -> &'static str {
    match op_result {
        OperationResult::OpInner(tr) => {
            use OperationResultTr::*;
            match tr {
                CreateAccount(r) => r.name(),
                Payment(r) => r.name(),
                PathPaymentStrictReceive(r) => r.name(),
                ManageSellOffer(r) | CreatePassiveSellOffer(r) => r.name(),
                SetOptions(r) => r.name(),
                ChangeTrust(r) => r.name(),
                AllowTrust(r) => r.name(),
                AccountMerge(r) => r.name(),
                Inflation(r) => r.name(),
                ManageData(r) => r.name(),
                BumpSequence(r) => r.name(),
                ManageBuyOffer(r) => r.name(),
                PathPaymentStrictSend(r) => r.name(),
                CreateClaimableBalance(r) => r.name(),
                ClaimClaimableBalance(r) => r.name(),
                BeginSponsoringFutureReserves(r) => r.name(),
                EndSponsoringFutureReserves(r) => r.name(),
                RevokeSponsorship(r) => r.name(),
                Clawback(r) => r.name(),
                ClawbackClaimableBalance(r) => r.name(),
                SetTrustLineFlags(r) => r.name(),
                LiquidityPoolDeposit(r) => r.name(),
                LiquidityPoolWithdraw(r) => r.name(),
                InvokeHostFunction(r) => r.name(),
                ExtendFootprintTtl(r) => r.name(),
                RestoreFootprint(r) => r.name(),
            }
        }
        other => other.name(),
    }
}

/// All claim atoms — order-book AND liquidity-pool — crossed by a successful op
/// result, in XDR vector order.
///
/// Covers every atom-bearing op kind: both path-payment variants (`offers`)
/// and all three offer ops — `ManageSellOffer` / `ManageBuyOffer` /
/// `CreatePassiveSellOffer` (`ManageOfferSuccessResult.offers_claimed`), which
/// can fill against an AMM (CAP-38 unified order-book + pool exchange). Empty
/// for any non-success branch and non-atom-bearing ops. This is the "unified
/// claim-atom extractor" of the 0261 decision — one generic pass captures every
/// crossing, with no second historical pass. Consumers: [`claim_lp_atoms`]
/// (pool-only, `poolIds` / `gross_volume_a`) and [`extract_counterparties`]
/// (crossed-offer sellers, task 0359 F-C).
pub(crate) fn claim_atoms(op_result: &OperationResult) -> &[ClaimAtom] {
    use OperationResultTr::*;
    match op_result {
        OperationResult::OpInner(PathPaymentStrictSend(PathPaymentStrictSendResult::Success(
            s,
        ))) => s.offers.as_slice(),
        OperationResult::OpInner(PathPaymentStrictReceive(
            PathPaymentStrictReceiveResult::Success(s),
        )) => s.offers.as_slice(),
        OperationResult::OpInner(
            ManageSellOffer(ManageSellOfferResult::Success(m))
            | CreatePassiveSellOffer(ManageSellOfferResult::Success(m)),
        ) => m.offers_claimed.as_slice(),
        OperationResult::OpInner(ManageBuyOffer(ManageBuyOfferResult::Success(m))) => {
            m.offers_claimed.as_slice()
        }
        _ => &[],
    }
}

/// Liquidity-pool claim atoms only — the `poolIds` / `gross_volume_a` consumer
/// (task 0261). Order-book fills are excluded (they carry no pool id).
fn claim_lp_atoms(op_result: &OperationResult) -> impl Iterator<Item = &ClaimLiquidityAtom> {
    claim_atoms(op_result).iter().filter_map(|a| match a {
        ClaimAtom::LiquidityPool(lp) => Some(lp),
        _ => None,
    })
}

/// Append `poolIds` + `claimedAtoms` to an op's details when its result shows
/// liquidity-pool crossings (path payments and offers; see [`claim_lp_atoms`]).
/// `poolIds` is the deduped crossed-pool list (first-crossing order);
/// `claimedAtoms` keeps every LP fill with its amounts so `gross_volume_a` per
/// (pool, ledger) can be computed downstream without a second parse pass
/// (tasks 0247/0266/0199). No-op for ops with no LP atoms, so it is safe to
/// call unconditionally for every op.
///
/// Each atom also carries `amountA` — the fill amount on the pool's **canonical
/// asset A** side. A pool's `assetA` is by definition the canonically-smaller
/// of its two assets (`assetA < assetB`, Stellar pool-param rule), and an
/// atom's `{asset_sold, asset_bought}` are exactly the pool's two assets, so
/// `amountA` is the amount on `min(asset_sold, asset_bought)` (XDR `Asset`
/// ordering = type, then code, then issuer) — no pool-definition lookup
/// needed. This is the `gross_volume_a` per-atom contribution (task 0266
/// Phase 2 / 0279): the worker sums `amountA` per `(pool, ledger)`.
fn append_pool_claims(details: &mut Value, op_result: Option<&OperationResult>) {
    let Some(op_result) = op_result else { return };
    let Value::Object(map) = details else { return };

    let mut pool_ids: Vec<String> = Vec::new();
    let mut claimed: Vec<Value> = Vec::new();
    for atom in claim_lp_atoms(op_result) {
        let id = hex::encode(atom.liquidity_pool_id.0.as_slice());
        // Canonical asset-A side: the smaller of the two assets carries A.
        let amount_a = if atom.asset_sold <= atom.asset_bought {
            atom.amount_sold
        } else {
            atom.amount_bought
        };
        claimed.push(json!({
            "poolId": &id,
            "assetSold": format_asset(&atom.asset_sold),
            "amountSold": atom.amount_sold,
            "assetBought": format_asset(&atom.asset_bought),
            "amountBought": atom.amount_bought,
            "amountA": amount_a,
        }));
        if !pool_ids.contains(&id) {
            pool_ids.push(id);
        }
    }
    if claimed.is_empty() {
        return;
    }
    map.insert("poolIds".into(), Value::from(pool_ids));
    map.insert("claimedAtoms".into(), Value::from(claimed));
}

/// Extract the Soroban return value from TransactionMeta, if present.
fn soroban_return_value(meta: &TransactionMeta) -> Option<ScVal> {
    match meta {
        TransactionMeta::V3(v3) => v3.soroban_meta.as_ref().map(|m| m.return_value.clone()),
        TransactionMeta::V4(v4) => v4
            .soroban_meta
            .as_ref()
            .and_then(|m| m.return_value.clone()),
        _ => None,
    }
}

/// The ledger changes of operation `op_idx` (0-based), index-aligned with the
/// envelope's operations. Task 0359 uses these for the meta grain — the assets
/// of ops that name a claimable balance / LP only by id. Empty for the out-of-
/// range index.
///
/// The `TransactionMeta` match is EXHAUSTIVE (no `_`) on purpose: a future
/// variant (e.g. a Protocol-24 `V5`) must break the build HERE, not silently
/// yield zero meta changes and drop every claim-CB / LP asset on those ledgers
/// (the meta grain is their ONLY source). The legacy pre-Soroban metas (V0–V2)
/// map to `&[]` because the 0359 backfill window starts at the Soroban go-live
/// (ledger 50,457,424 / Protocol 20 / `V3`+), so V0..V2 never occur in the ingest
/// window. NB this is a WINDOW guarantee, not a protocol one: claimable balances
/// (Protocol 15) and liquidity pools (Protocol 18) predate `V3`, so their CB/LP
/// ops on P15..P19 ledgers live in `V2` meta -- a pre-P20 ingest would need `V2`
/// handling here or it would silently drop those meta-grain assets.
fn op_meta_changes(tx_meta: Option<&TransactionMeta>, op_idx: usize) -> &[LedgerEntryChange] {
    match tx_meta {
        None | Some(TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_)) => {
            &[]
        }
        Some(TransactionMeta::V3(v3)) => v3
            .operations
            .get(op_idx)
            .map(|m| m.changes.as_slice())
            .unwrap_or(&[]),
        Some(TransactionMeta::V4(v4)) => v4
            .operations
            .get(op_idx)
            .map(|m| m.changes.as_slice())
            .unwrap_or(&[]),
    }
}

/// Extract operation type discriminator and details JSON for a single
/// operation. Matches the XDR body variant; the resulting `OperationType`
/// casts to SMALLINT via `#[repr(i16)]` with zero lookup cost.
fn extract_op_details(
    body: &OperationBody,
    return_value: Option<&ScVal>,
    op_result: Option<&OperationResult>,
    op_changes: &[LedgerEntryChange],
    _ledger_sequence: u32,
    _tx_index: usize,
    _op_index: usize,
) -> (OperationType, Value) {
    // Per-op body → (type, details). `append_pool_claims` runs once after the
    // match: it no-ops unless the op result carries LP claim atoms, so path
    // payments AND offers crossing a pool (task 0261) get `poolIds`/
    // `claimedAtoms` without per-arm plumbing.
    let (op_type, mut details) = match body {
        OperationBody::CreateAccount(op) => (
            OperationType::CreateAccount,
            json!({
                "destination": op.destination.0.to_string(),
                "startingBalance": op.starting_balance,
            }),
        ),
        OperationBody::Payment(op) => (
            OperationType::Payment,
            json!({
                "destination": muxed_to_g_strkey(&op.destination),
                "asset": format_asset(&op.asset),
                "amount": op.amount,
            }),
        ),
        OperationBody::PathPaymentStrictReceive(op) => (
            OperationType::PathPaymentStrictReceive,
            json!({
                "sendAsset": format_asset(&op.send_asset),
                "sendMax": op.send_max,
                "destination": muxed_to_g_strkey(&op.destination),
                "destAsset": format_asset(&op.dest_asset),
                "destAmount": op.dest_amount,
                "path": op.path.iter().map(format_asset).collect::<Vec<_>>(),
            }),
        ),
        OperationBody::PathPaymentStrictSend(op) => (
            OperationType::PathPaymentStrictSend,
            json!({
                "sendAsset": format_asset(&op.send_asset),
                "sendAmount": op.send_amount,
                "destination": muxed_to_g_strkey(&op.destination),
                "destAsset": format_asset(&op.dest_asset),
                "destMin": op.dest_min,
                "path": op.path.iter().map(format_asset).collect::<Vec<_>>(),
            }),
        ),
        OperationBody::ManageSellOffer(op) => (
            OperationType::ManageSellOffer,
            json!({
                "selling": format_asset(&op.selling),
                "buying": format_asset(&op.buying),
                "amount": op.amount,
                "price": format_price(&op.price),
                "offerId": op.offer_id,
            }),
        ),
        OperationBody::ManageBuyOffer(op) => (
            OperationType::ManageBuyOffer,
            json!({
                "selling": format_asset(&op.selling),
                "buying": format_asset(&op.buying),
                "buyAmount": op.buy_amount,
                "price": format_price(&op.price),
                "offerId": op.offer_id,
            }),
        ),
        OperationBody::CreatePassiveSellOffer(op) => (
            OperationType::CreatePassiveSellOffer,
            json!({
                "selling": format_asset(&op.selling),
                "buying": format_asset(&op.buying),
                "amount": op.amount,
                "price": format_price(&op.price),
            }),
        ),
        OperationBody::SetOptions(op) => {
            let mut details = serde_json::Map::new();
            if let Some(ref dest) = op.inflation_dest {
                details.insert("inflationDest".into(), json!(dest.0.to_string()));
            }
            if let Some(flags) = op.clear_flags {
                details.insert("clearFlags".into(), json!(flags));
            }
            if let Some(flags) = op.set_flags {
                details.insert("setFlags".into(), json!(flags));
            }
            if let Some(w) = op.master_weight {
                details.insert("masterWeight".into(), json!(w));
            }
            if let Some(t) = op.low_threshold {
                details.insert("lowThreshold".into(), json!(t));
            }
            if let Some(t) = op.med_threshold {
                details.insert("medThreshold".into(), json!(t));
            }
            if let Some(t) = op.high_threshold {
                details.insert("highThreshold".into(), json!(t));
            }
            if let Some(ref domain) = op.home_domain {
                let s = std::str::from_utf8(domain.as_vec()).unwrap_or("<invalid-utf8>");
                details.insert("homeDomain".into(), json!(s));
            }
            if let Some(ref signer) = op.signer {
                details.insert("signerKey".into(), json!(signer.key.to_string()));
                details.insert("signerWeight".into(), json!(signer.weight));
            }
            (OperationType::SetOptions, Value::Object(details))
        }
        OperationBody::ChangeTrust(op) => (
            OperationType::ChangeTrust,
            json!({
                "asset": format_change_trust_asset(&op.line),
                "limit": op.limit,
            }),
        ),
        OperationBody::AllowTrust(op) => (
            OperationType::AllowTrust,
            json!({
                "trustor": op.trustor.0.to_string(),
                "asset": format_asset_code(&op.asset),
                "authorize": op.authorize,
            }),
        ),
        OperationBody::AccountMerge(destination) => (
            OperationType::AccountMerge,
            json!({
                "destination": muxed_to_g_strkey(destination),
            }),
        ),
        OperationBody::Inflation => (OperationType::Inflation, json!({})),
        OperationBody::ManageData(op) => {
            let name = std::str::from_utf8(op.data_name.as_vec()).unwrap_or("<invalid-utf8>");
            let value = op.data_value.as_ref().map(|v| {
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, v.as_slice())
            });
            (
                OperationType::ManageData,
                json!({
                    "name": name,
                    "value": value,
                }),
            )
        }
        OperationBody::BumpSequence(op) => (
            OperationType::BumpSequence,
            json!({
                "bumpTo": op.bump_to.0,
            }),
        ),
        OperationBody::CreateClaimableBalance(op) => (
            OperationType::CreateClaimableBalance,
            json!({
                "asset": format_asset(&op.asset),
                "amount": op.amount,
                // Task 0460 #16: the full claimant vec — the addresses are
                // the point of the operation. One shape, no summary count
                // beside it (derive length where needed).
                "claimants": op.claimants.iter().map(claimant_json).collect::<Vec<_>>(),
            }),
        ),
        OperationBody::ClaimClaimableBalance(op) => (
            OperationType::ClaimClaimableBalance,
            cb_details(op_changes, &op.balance_id),
        ),
        OperationBody::BeginSponsoringFutureReserves(op) => (
            OperationType::BeginSponsoringFutureReserves,
            json!({
                "sponsoredId": op.sponsored_id.0.to_string(),
            }),
        ),
        OperationBody::EndSponsoringFutureReserves => {
            (OperationType::EndSponsoringFutureReserves, json!({}))
        }
        OperationBody::RevokeSponsorship(op) => {
            let details = match op {
                RevokeSponsorshipOp::LedgerEntry(key) => json!({
                    "kind": "ledgerEntry",
                    "ledgerKeyType": key.name(),
                }),
                RevokeSponsorshipOp::Signer(s) => json!({
                    "kind": "signer",
                    "accountId": s.account_id.0.to_string(),
                    "signerKey": s.signer_key.to_string(),
                }),
            };
            (OperationType::RevokeSponsorship, details)
        }
        OperationBody::Clawback(op) => (
            OperationType::Clawback,
            json!({
                "asset": format_asset(&op.asset),
                "from": muxed_to_g_strkey(&op.from),
                "amount": op.amount,
            }),
        ),
        OperationBody::ClawbackClaimableBalance(op) => (
            OperationType::ClawbackClaimableBalance,
            cb_details(op_changes, &op.balance_id),
        ),
        OperationBody::SetTrustLineFlags(op) => (
            OperationType::SetTrustLineFlags,
            json!({
                "trustor": op.trustor.0.to_string(),
                "asset": format_asset(&op.asset),
                "clearFlags": op.clear_flags,
                "setFlags": op.set_flags,
            }),
        ),
        OperationBody::LiquidityPoolDeposit(op) => {
            let mut details = json!({
                "liquidityPoolId": hex::encode(op.liquidity_pool_id.0.as_slice()),
                "maxAmountA": op.max_amount_a,
                "maxAmountB": op.max_amount_b,
                "minPrice": format_price(&op.min_price),
                "maxPrice": format_price(&op.max_price),
            });
            if let Some(delta) = pool_delta_details(op_changes, &op.liquidity_pool_id) {
                details["poolDelta"] = delta;
            }
            (OperationType::LiquidityPoolDeposit, details)
        }
        OperationBody::LiquidityPoolWithdraw(op) => {
            let mut details = json!({
                "liquidityPoolId": hex::encode(op.liquidity_pool_id.0.as_slice()),
                "amount": op.amount,
                "minAmountA": op.min_amount_a,
                "minAmountB": op.min_amount_b,
            });
            if let Some(delta) = pool_delta_details(op_changes, &op.liquidity_pool_id) {
                details["poolDelta"] = delta;
            }
            (OperationType::LiquidityPoolWithdraw, details)
        }
        OperationBody::InvokeHostFunction(op) => {
            let details = extract_invoke_host_function(op, return_value);
            (OperationType::InvokeHostFunction, details)
        }
        OperationBody::ExtendFootprintTtl(op) => (
            OperationType::ExtendFootprintTtl,
            json!({
                "extendTo": op.extend_to,
            }),
        ),
        OperationBody::RestoreFootprint(_) => (OperationType::RestoreFootprint, json!({})),
    };
    append_pool_claims(&mut details, op_result);
    (op_type, details)
}

/// Extract enriched details for INVOKE_HOST_FUNCTION operations.
fn extract_invoke_host_function(op: &InvokeHostFunctionOp, return_value: Option<&ScVal>) -> Value {
    match &op.host_function {
        HostFunction::InvokeContract(args) => {
            let contract_id = args.contract_address.to_string();
            let function_name =
                std::str::from_utf8(args.function_name.as_vec()).unwrap_or("<invalid-utf8>");
            let function_args: Vec<Value> = args.args.iter().map(scval_to_typed_json).collect();
            let ret = return_value.map(scval_to_typed_json);
            json!({
                "hostFunctionType": "invokeContract",
                "contractId": contract_id,
                "functionName": function_name,
                "functionArgs": function_args,
                "returnValue": ret,
            })
        }
        HostFunction::CreateContract(args) => {
            json!({
                "hostFunctionType": "createContract",
                "executable": format_contract_executable(&args.executable),
            })
        }
        HostFunction::UploadContractWasm(wasm) => {
            json!({
                "hostFunctionType": "uploadContractWasm",
                "wasmLength": wasm.len(),
            })
        }
        HostFunction::CreateContractV2(args) => {
            json!({
                "hostFunctionType": "createContractV2",
                "executable": format_contract_executable(&args.executable),
                "constructorArgs": args.constructor_args.iter().map(scval_to_typed_json).collect::<Vec<_>>(),
            })
        }
    }
}

// --- Formatting helpers ---

fn format_asset(asset: &Asset) -> Value {
    match asset {
        Asset::Native => json!("native"),
        Asset::CreditAlphanum4(a) => {
            let code = crate::asset_code::asset_code_str(a.asset_code.as_slice());
            json!(format!("{}:{}", code, a.issuer.0.to_string()))
        }
        Asset::CreditAlphanum12(a) => {
            let code = crate::asset_code::asset_code_str(a.asset_code.as_slice());
            json!(format!("{}:{}", code, a.issuer.0.to_string()))
        }
    }
}

fn format_change_trust_asset(asset: &ChangeTrustAsset) -> Value {
    match asset {
        ChangeTrustAsset::Native => json!("native"),
        ChangeTrustAsset::CreditAlphanum4(a) => {
            let code = crate::asset_code::asset_code_str(a.asset_code.as_slice());
            json!(format!("{}:{}", code, a.issuer.0.to_string()))
        }
        ChangeTrustAsset::CreditAlphanum12(a) => {
            let code = crate::asset_code::asset_code_str(a.asset_code.as_slice());
            json!(format!("{}:{}", code, a.issuer.0.to_string()))
        }
        ChangeTrustAsset::PoolShare(params) => {
            json!({ "type": "liquidityPool", "params": params.name() })
        }
    }
}

fn format_asset_code(code: &AssetCode) -> Value {
    let s = crate::asset_code::asset_code_str(crate::asset_code::asset_code_bytes(code));
    json!(s)
}

fn format_price(price: &Price) -> Value {
    json!({ "n": price.n, "d": price.d })
}

fn format_claimable_balance_id(id: &ClaimableBalanceId) -> Value {
    match id {
        ClaimableBalanceId::ClaimableBalanceIdTypeV0(hash) => {
            json!(hex::encode(hash.0))
        }
    }
}

/// Claimant → `{destination, predicate}` (task 0460 #16).
fn claimant_json(claimant: &Claimant) -> Value {
    let Claimant::ClaimantTypeV0(v0) = claimant;
    json!({
        "destination": v0.destination.0.to_string(),
        "predicate": claim_predicate_json(&v0.predicate),
    })
}

/// The full recursive claim predicate as tagged JSON — no lossy summary;
/// `and`/`or` carry up to 2 sub-predicates by XDR definition.
fn claim_predicate_json(predicate: &ClaimPredicate) -> Value {
    match predicate {
        ClaimPredicate::Unconditional => json!({ "type": "unconditional" }),
        ClaimPredicate::And(ps) => json!({
            "type": "and",
            "predicates": ps.iter().map(claim_predicate_json).collect::<Vec<_>>(),
        }),
        ClaimPredicate::Or(ps) => json!({
            "type": "or",
            "predicates": ps.iter().map(claim_predicate_json).collect::<Vec<_>>(),
        }),
        ClaimPredicate::Not(inner) => json!({
            "type": "not",
            "predicate": inner.as_deref().map(claim_predicate_json),
        }),
        ClaimPredicate::BeforeAbsoluteTime(t) => json!({
            "type": "beforeAbsoluteTime",
            "timePoint": t,
        }),
        ClaimPredicate::BeforeRelativeTime(secs) => json!({
            "type": "beforeRelativeTime",
            "seconds": secs,
        }),
    }
}

/// Details for claim/clawback-claimable-balance — shared by both arms. The
/// body carries only the id; the asset + amount live in the same-op ledger
/// entry (task 0453 D8). Keys are optional — absent when the meta lacks the
/// entry, never guessed.
fn cb_details(op_changes: &[LedgerEntryChange], balance_id: &ClaimableBalanceId) -> Value {
    let mut d = json!({
        "balanceId": format_claimable_balance_id(balance_id),
    });
    if let Some((asset, amount)) =
        crate::asset_appearances::claimed_cb_asset_amount(op_changes, balance_id)
    {
        d["asset"] = json!(format_asset(&asset));
        d["amount"] = json!(amount);
    }
    d
}

/// The constant-product body of `entry`, when it is the pool `pool_id`.
/// Matching on the id (not the first pool entry) mirrors `lp_pool_assets`.
fn pool_constant_product<'a>(
    entry: &'a LedgerEntry,
    pool_id: &PoolId,
) -> Option<&'a LiquidityPoolEntryConstantProduct> {
    match &entry.data {
        LedgerEntryData::LiquidityPool(lp) if &lp.liquidity_pool_id == pool_id => {
            let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &lp.body;
            Some(cp)
        }
        _ => None,
    }
}

/// What an LP deposit / withdraw actually moved, read from the operation's OWN
/// ledger changes (task 0279).
///
/// The body carries only the caller's bounds — `maxAmountA`/`maxAmountB` on a
/// deposit, `amount` + `minAmountA`/`minAmountB` on a withdrawal — never what
/// executed. The pool entry's before/after images are the only record of that,
/// which is why these ops have no claim atoms to read instead. Same contract as
/// [`cb_details`]: `None` when the meta lacks the entry, never guessed.
///
/// `amountA` / `amountB` are AFTER MINUS BEFORE, i.e. **signed from the pool's
/// side** — a deposit reads `+/+`, a withdrawal `-/-` — which is the sign the
/// claim-atom consumers already use for trades, so one downstream shape covers
/// all three event kinds (`lp_operation_amounts`). The boundary cases fall out
/// of the same subtraction: a pool created by its first deposit has no `state`
/// pre-image (before = 0) and one emptied by its last withdrawal is `Removed`
/// with no post-image (after = 0).
///
/// Coverage follows [`op_meta_changes`]: V0–V2 metas yield no changes, so this
/// is silent on pre-Protocol-20 ledgers — outside the ingest window, which
/// starts at the Soroban go-live.
fn pool_delta_details(op_changes: &[LedgerEntryChange], pool_id: &PoolId) -> Option<Value> {
    let mut before = None;
    let mut after = None;
    let mut removed = false;
    for change in op_changes {
        match change {
            // FIRST match wins for the pre-image (`before.or(new)`), LAST for
            // the post-image (`new.or(after)`) — the two ends of the op. The
            // asymmetry only shows when one op touches a pool repeatedly, as a
            // path payment does under CAP-38 interleaved matching (a
            // State/Updated pair per fill): taking the last `State` there would
            // measure the final fill instead of the whole operation. Deposits
            // and withdrawals touch it once, so today both readings agree.
            LedgerEntryChange::State(e) => before = before.or(pool_constant_product(e, pool_id)),
            LedgerEntryChange::Created(e)
            | LedgerEntryChange::Updated(e)
            | LedgerEntryChange::Restored(e) => after = pool_constant_product(e, pool_id).or(after),
            LedgerEntryChange::Removed(LedgerKey::LiquidityPool(k)) => {
                removed |= &k.liquidity_pool_id == pool_id;
            }
            LedgerEntryChange::Removed(_) => {}
        }
    }

    // Either image identifies the pool's two assets; they never change.
    let params = &after.or(before)?.params;
    let (before_a, before_b) = before.map_or((0, 0), |cp| (cp.reserve_a, cp.reserve_b));
    let (after_a, after_b) = match (removed, after) {
        (true, _) => (0, 0),
        (false, Some(cp)) => (cp.reserve_a, cp.reserve_b),
        // A pre-image with nothing after it means this op did not move the
        // pool (a `state`-only read). Nothing to report.
        (false, None) => return None,
    };

    let (delta_a, delta_b) = (after_a - before_a, after_b - before_b);
    if delta_a == 0 && delta_b == 0 {
        return None;
    }
    Some(json!({
        "poolId": hex::encode(pool_id.0.as_slice()),
        "assetA": format_asset(&params.asset_a),
        "amountA": delta_a,
        "assetB": format_asset(&params.asset_b),
        "amountB": delta_b,
    }))
}

fn format_contract_executable(exec: &ContractExecutable) -> Value {
    match exec {
        ContractExecutable::Wasm(hash) => json!({ "type": "wasm", "hash": hex::encode(hash.0) }),
        ContractExecutable::StellarAsset => json!({ "type": "stellar_asset" }),
        // CAP-85 (protocol 28): code owned by another contract — no own hash.
        ContractExecutable::ExternalRef(r) => json!({
            "type": "external_ref",
            "owner": r.executable_owner.to_string(),
            "tag": std::str::from_utf8(r.tag.0.as_slice()).unwrap_or("<invalid-utf8>"),
        }),
    }
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
