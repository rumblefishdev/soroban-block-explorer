//! NFT rows staged per ledger: the per-contract routing verdict and the
//! `nfts` / `nfts_pending` and `nft_ownership` / `nft_ownership_pending` rows
//! it routes (task 0217 / 0220).
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::HashMap;

use domain::ContractType;
use xdr_parser::types::{ExtractedNft, ExtractedNftEvent};

use super::{StagedLedger, staging_err};
use crate::SchemaError;
use crate::persist::ids;
use crate::persist::rows::{NftOwnershipPendingRow, NftOwnershipRow, NftPendingRow, NftRow};

pub(super) fn nft_rows(
    out: &mut StagedLedger,
    nfts: &[ExtractedNft],
    nft_events: &[ExtractedNftEvent],
    prior_contract_verdicts: &HashMap<String, ContractType>,
    tx_id_by_hash: &HashMap<String, i64>,
) -> Result<(), SchemaError> {
    // ---- NFT routing verdict map (task 0217 / 0220) -------------------
    //
    // Build a per-contract verdict map keyed by strkey. Sources, in
    // precedence order:
    //   1. Same-ledger `contract_rows` carrying a definitive
    //      `contract_type` (Token / Nft / Fungible). Either:
    //        - SAC deploy (`is_sac=true` → Token).
    //        - WASM-classified deploy (the override applied above).
    //   2. SAC overrides (also Token) — these were skipped from Pass-2
    //      stubs, so they're in `out.contract_rows` already.
    // Contracts with NO entry in EITHER source → treat as `Other`/uncached →
    // route to pending. The stage itself has no DB access; cross-ledger
    // verdicts arrive via `prior_contract_verdicts` (task 0283 live G9), the
    // writer's lookup of `soroban_contracts` for contracts emitting NFT
    // rows/events here but deployed earlier. This restores the PG
    // `ClassificationCache` semantic the CH cutover dropped — without it a
    // later transfer from an already-classified NFT would quarantine.
    let mut verdict_by_contract: HashMap<&str, ContractType> = HashMap::new();
    for row in &out.contract_rows {
        if let Some(ty_i16) = row.contract_type
            && let Ok(ty) = ContractType::try_from(ty_i16)
        {
            verdict_by_contract.insert(row.contract_id.as_str(), ty);
        }
    }

    // 3-way routing helper. Mirrors PG `resolve_nft_filter` bucketing.
    // This-ledger `contract_rows` take precedence; `prior_contract_verdicts`
    // (G9, cross-ledger) is the fallback for contracts not deployed here.
    enum NftRoute {
        Hot,
        Pending,
        Drop,
    }
    let route_for = |strkey: &str| -> NftRoute {
        let verdict = verdict_by_contract
            .get(strkey)
            .copied()
            .or_else(|| prior_contract_verdicts.get(strkey).copied());
        match verdict {
            Some(ContractType::Token) | Some(ContractType::Fungible) => NftRoute::Drop,
            Some(ContractType::Nft) => NftRoute::Hot,
            // `Other` and uncached (no entry in either source) both go to
            // quarantine — same semantic as PG-side `resolve_nft_filter`.
            _ => NftRoute::Pending,
        }
    };

    // ---- nfts / nfts_pending (dedup by (contract_id, token_id),
    //                            latest watermark) ----
    //
    // Each `(contract_id, token_id)` row lives in exactly one bucket
    // (hot OR pending) per partition — picked by the per-contract
    // verdict above. Dedup keys are per-bucket so a contract that
    // somehow appeared with mixed verdicts within the same ledger
    // (impossible today, defensive) would have separate slots.
    let mut nft_hot_indices: HashMap<(i64, String), usize> = HashMap::new();
    let mut nft_pending_indices: HashMap<(i64, String), usize> = HashMap::new();
    for nft in nfts {
        let route = route_for(nft.contract_id.as_str());
        if matches!(route, NftRoute::Drop) {
            continue;
        }
        let contract_id_int = ids::contract_id(&nft.contract_id);
        let watermark = i64::from(nft.last_seen_ledger);
        let key = (contract_id_int, nft.token_id.clone());
        let owner_id = nft.owner_account.as_deref().map(ids::account_id);
        let minted = nft.minted_at_ledger.map(i64::from);

        match route {
            NftRoute::Hot => match nft_hot_indices.get(&key).copied() {
                Some(idx) => {
                    let existing = &mut out.nft_rows[idx];
                    if watermark >= existing.current_owner_ledger {
                        existing.current_owner_id = owner_id;
                        existing.current_owner_ledger = watermark;
                    }
                    existing.minted_at_ledger = match (existing.minted_at_ledger, minted) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) => Some(a),
                        (None, b) => b,
                    };
                    existing.collection_name = existing
                        .collection_name
                        .clone()
                        .or_else(|| nft.collection_name.clone());
                    existing.name = existing.name.clone().or_else(|| nft.name.clone());
                    existing.media_url =
                        existing.media_url.clone().or_else(|| nft.media_url.clone());
                }
                None => {
                    nft_hot_indices.insert(key, out.nft_rows.len());
                    out.nft_rows.push(NftRow {
                        contract_id: contract_id_int,
                        token_id: nft.token_id.clone(),
                        collection_name: nft.collection_name.clone(),
                        name: nft.name.clone(),
                        media_url: nft.media_url.clone(),
                        minted_at_ledger: minted,
                        current_owner_id: owner_id,
                        current_owner_ledger: watermark,
                    });
                }
            },
            NftRoute::Pending => match nft_pending_indices.get(&key).copied() {
                Some(idx) => {
                    let existing = &mut out.nft_pending_rows[idx];
                    if watermark >= existing.current_owner_ledger {
                        existing.current_owner_id = owner_id;
                        existing.current_owner_ledger = watermark;
                    }
                    existing.minted_at_ledger = match (existing.minted_at_ledger, minted) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) => Some(a),
                        (None, b) => b,
                    };
                    existing.collection_name = existing
                        .collection_name
                        .clone()
                        .or_else(|| nft.collection_name.clone());
                    existing.name = existing.name.clone().or_else(|| nft.name.clone());
                    existing.media_url =
                        existing.media_url.clone().or_else(|| nft.media_url.clone());
                }
                None => {
                    nft_pending_indices.insert(key, out.nft_pending_rows.len());
                    out.nft_pending_rows.push(NftPendingRow {
                        contract_id: contract_id_int,
                        token_id: nft.token_id.clone(),
                        collection_name: nft.collection_name.clone(),
                        name: nft.name.clone(),
                        media_url: nft.media_url.clone(),
                        minted_at_ledger: minted,
                        current_owner_id: owner_id,
                        current_owner_ledger: watermark,
                    });
                }
            },
            NftRoute::Drop => unreachable!("filtered above"),
        }
    }

    // ---- nft_ownership / nft_ownership_pending ----
    for ev in nft_events {
        let route = route_for(ev.contract_id.as_str());
        if matches!(route, NftRoute::Drop) {
            continue;
        }
        let Some(&tx_id) = tx_id_by_hash.get(&ev.transaction_hash) else {
            continue;
        };
        let event_order =
            i16::try_from(ev.event_order).map_err(|_| staging_err("nft event_order overflow"))?;
        let contract_id = ids::contract_id(&ev.contract_id);
        let ledger_sequence = i64::from(ev.ledger_sequence);
        let owner_id = ev.owner_account.as_deref().map(ids::account_id);
        let event_type = ev.event_type as i16;

        match route {
            NftRoute::Hot => out.nft_ownership_rows.push(NftOwnershipRow {
                contract_id,
                token_id: ev.token_id.clone(),
                ledger_sequence,
                event_order,
                transaction_id: tx_id,
                owner_id,
                event_type,
            }),
            NftRoute::Pending => out.nft_ownership_pending_rows.push(NftOwnershipPendingRow {
                contract_id,
                token_id: ev.token_id.clone(),
                ledger_sequence,
                event_order,
                transaction_id: tx_id,
                owner_id,
                event_type,
            }),
            NftRoute::Drop => unreachable!("filtered above"),
        }
    }

    Ok(())
}
