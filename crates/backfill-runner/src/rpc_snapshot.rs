//! Soroban RPC client for the bootstrap-snapshot account-state path
//! (task 0214, audit §E06).
//!
//! ## Scope
//!
//! The CH writer's `accounts` table populates correctly only when the
//! parser observes a `LedgerEntryAccount` / `LedgerEntryTrustLine`
//! change in the indexed ledger range. For accounts that appear in the
//! backfill window only as `transaction_participants`, the actual
//! state (`sequence_number`, `home_domain`, `account_balances_current`)
//! stays at the staging defaults (`0` / `null` / empty). In short
//! 64 k-ledger windows that is the majority of participants.
//!
//! This module is a thin JSON-RPC client over Stellar's
//! `getLedgerEntries` endpoint. The bootstrap runner step
//! (`crate::bootstrap`) discovers every G-StrKey referenced in the
//! window, asks Soroban RPC for the **live (current-ledger)**
//! `AccountEntry` (and matched `TrustLineEntry`s) of each account,
//! and stages those rows alongside the parser-emitted ones so the
//! persist path treats both uniformly. Note: `getLedgerEntries` does
//! not accept a ledger boundary — it returns whatever the RPC node's
//! current head is at request time. The bootstrap is therefore best
//! run shortly after the window completes; for backfill replays of
//! older windows, the snapshot will be "newer than window", which is
//! still strictly better than the parser-only skeleton.
//!
//! ## Why a private module, not a new crate
//!
//! Task 0214 is the first consumer of Soroban RPC in the workspace.
//! Tasks 0218 (SAC override) and 0219 (classic-credit assets) carry
//! cheaper non-RPC layers and only fall back to RPC for stragglers.
//! Until a second concrete consumer lands, an inline module under
//! `crates/backfill-runner` is the smallest blast radius — no new
//! crate boilerplate, no public surface to maintain. The
//! refactor-to-a-shared-crate is a one-day move if a second consumer
//! appears (Option B in the task body). Decision recorded in the 0214
//! task history.
//!
//! ## Rate limits + concurrency
//!
//! Soroban RPC rate-limits per-IP. Empirical sustained throughput is
//! ~50 req/s with ~200 keys per request → ~10 k keys / s. A 64 k-ledger
//! window referencing 100 k accounts plus their trustlines therefore
//! costs roughly 30–60 s of wall time on the bootstrap step. **Today
//! the client batches sequentially** (one in-flight RPC at a time) —
//! the per-batch size cap is [`MAX_KEYS_PER_REQUEST`] (200, matching
//! mainnet RPC's advertised soft limit). A parallel-request knob
//! (`--rpc-concurrency`) is a likely future refinement once we have
//! real throughput numbers from a CH pilot run; the task body §"Out
//! of Scope" tracks it.
//!
//! ## What the response looks like
//!
//! Soroban RPC returns one `LedgerEntryData` blob per requested key
//! (or omits the key entirely if the entry does not exist). The blob
//! is base64-encoded XDR of `LedgerEntryData` from the `stellar-xdr`
//! crate. Successful decode yields:
//!
//! - `AccountEntry { account_id, balance, seq_num, home_domain, ... }`
//!   for `LedgerKey::Account` requests.
//! - `TrustLineEntry { account_id, asset, balance, flags, ... }` for
//!   `LedgerKey::Trustline` requests.
//!
//! Missing entries are silently dropped: an account that exists in
//! `transaction_participants` but has no current `AccountEntry`
//! (closed account, ledger key TTL'd, never funded) is treated as
//! "snapshot has nothing to add" — the parser-emitted skeleton row
//! survives.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stellar_xdr::curr::{
    AccountId, ContractDataDurability, ContractId, Hash, LedgerEntryData, LedgerKey,
    LedgerKeyAccount, LedgerKeyContractCode, LedgerKeyContractData, LedgerKeyTrustLine, Limits,
    PublicKey, ReadXdr, ScAddress, ScSymbol, ScVal, ScVec, TrustLineAsset, Uint256, WriteXdr,
};
use tracing::warn;

/// Maximum keys per `getLedgerEntries` JSON-RPC call.
///
/// Soroban RPC advertises a soft cap around 200. Going higher
/// returns `code=-32602 invalid params` from the public endpoints
/// even when the underlying core supports more — keep us inside the
/// documented envelope.
pub const MAX_KEYS_PER_REQUEST: usize = 200;

/// Sustained-throughput hint for the bootstrap runner step.
///
/// Public Soroban RPC instances rate-limit roughly to 50 RPS / IP. The
/// runner uses this as a default-concurrency starting point only;
/// real throughput is observed and recorded on the task's
/// "empirical replay" acceptance criterion.
///
/// Unused today — the bootstrap step batches sequentially under
/// [`MAX_KEYS_PER_REQUEST`] keys per call. A future revision adds a
/// concurrent worker pool here.
#[allow(dead_code)]
pub const DEFAULT_CONCURRENCY: usize = 4;

/// HTTP request timeout for one `getLedgerEntries` call. Generous to
/// absorb tail latency from a busy mainnet RPC without aborting an
/// otherwise-successful batch.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Soroban RPC client. Wraps a long-lived `reqwest::Client` so
/// connection pooling persists across batches.
#[derive(Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    endpoint: String,
}

impl RpcClient {
    /// Build a client pointed at `endpoint` (full URL including scheme,
    /// e.g. `https://soroban-rpc.mainnet.stellar.gateway.fm`).
    pub fn new(endpoint: impl Into<String>) -> Result<Self, RpcError> {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent("soroban-block-explorer/backfill-runner/0.1")
            .build()
            .map_err(RpcError::Http)?;
        Ok(Self {
            http,
            endpoint: endpoint.into(),
        })
    }

    /// Fetch the current live state of `keys` from Soroban RPC. Missing
    /// keys are silently dropped from the response — callers receive
    /// only the entries the network actually has.
    ///
    /// Splits `keys` into batches of [`MAX_KEYS_PER_REQUEST`] under the
    /// hood; callers can hand in an arbitrary number of keys and trust
    /// the client not to overshoot the RPC's soft cap.
    pub async fn get_ledger_entries(
        &self,
        keys: &[LedgerKey],
    ) -> Result<Vec<LedgerEntryRecord>, RpcError> {
        let mut out = Vec::with_capacity(keys.len());
        for chunk in keys.chunks(MAX_KEYS_PER_REQUEST) {
            let batch = self.get_ledger_entries_batch(chunk).await?;
            out.extend(batch);
        }
        Ok(out)
    }

    /// One JSON-RPC call. Caller is responsible for keeping `keys` at
    /// or below [`MAX_KEYS_PER_REQUEST`].
    async fn get_ledger_entries_batch(
        &self,
        keys: &[LedgerKey],
    ) -> Result<Vec<LedgerEntryRecord>, RpcError> {
        let encoded: Vec<String> = keys
            .iter()
            .map(|k| {
                k.to_xdr(Limits::none())
                    .map(|bytes| BASE64.encode(&bytes))
                    .map_err(|e| RpcError::EncodeKey(e.to_string()))
            })
            .collect::<Result<_, _>>()?;

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "getLedgerEntries",
            params: GetLedgerEntriesParams { keys: encoded },
        };

        let resp = self
            .http
            .post(&self.endpoint)
            .json(&req)
            .send()
            .await
            .map_err(RpcError::Http)?;

        let status = resp.status();
        let body = resp.text().await.map_err(RpcError::Http)?;
        if !status.is_success() {
            return Err(RpcError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: JsonRpcResponse = serde_json::from_str(&body)
            .map_err(|e| RpcError::Decode(format!("response JSON: {e} body: {body}")))?;
        if let Some(err) = parsed.error {
            return Err(RpcError::Rpc {
                code: err.code,
                message: err.message,
            });
        }
        let result = parsed
            .result
            .ok_or_else(|| RpcError::Decode("response missing both `result` and `error`".into()))?;

        let entries = result.entries.unwrap_or_default();
        let mut out = Vec::with_capacity(entries.len());
        for raw in entries {
            let key_bytes = BASE64
                .decode(raw.key.as_bytes())
                .map_err(|e| RpcError::Decode(format!("key base64: {e}")))?;
            let key = LedgerKey::from_xdr(&key_bytes, Limits::none())
                .map_err(|e| RpcError::Decode(format!("key xdr: {e}")))?;

            let xdr_bytes = BASE64
                .decode(raw.xdr.as_bytes())
                .map_err(|e| RpcError::Decode(format!("entry base64: {e}")))?;
            let data = LedgerEntryData::from_xdr(&xdr_bytes, Limits::none())
                .map_err(|e| RpcError::Decode(format!("entry xdr: {e}")))?;

            out.push(LedgerEntryRecord {
                key,
                data,
                last_modified_ledger: raw.last_modified_ledger_seq,
            });
        }
        Ok(out)
    }
}

/// One decoded live entry from a `getLedgerEntries` response.
///
/// `data` is the `LedgerEntryData` XDR variant — for the bootstrap
/// path the caller pattern-matches on `Account(...)` and
/// `Trustline(...)`. Other variants are dropped by the bootstrap
/// (we only request Account / Trustline keys, but the type is
/// generic enough that a future caller could reuse this for any
/// `LedgerKey` shape).
///
/// `key` and `last_modified_ledger` are unused by the current
/// bootstrap caller (which only needs `data`) but kept on the
/// public surface — both are needed for the trustline-pairing pass
/// (`rebuild_trustline_asset` ↔ owner StrKey) we leave as
/// follow-up.
#[derive(Debug, Clone)]
pub struct LedgerEntryRecord {
    #[allow(dead_code)]
    pub key: LedgerKey,
    pub data: LedgerEntryData,
    #[allow(dead_code)]
    pub last_modified_ledger: u32,
}

/// Build a `LedgerKey::Account` from a G-StrKey. Returns `None` and
/// logs a warn on malformed input — bootstrap discovery shouldn't
/// crash the runner if one bad row sneaks in.
pub fn account_ledger_key(strkey: &str) -> Option<LedgerKey> {
    let pk = match stellar_strkey::ed25519::PublicKey::from_string(strkey) {
        Ok(pk) => pk,
        Err(err) => {
            warn!(strkey, %err, "rpc_snapshot: invalid G-StrKey; skipping");
            return None;
        }
    };
    let account_id = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk.0)));
    Some(LedgerKey::Account(LedgerKeyAccount { account_id }))
}

/// Build a `LedgerKey::ContractCode` from a 32-byte WASM hash (task 0327).
/// Used by the `upgradeable-backfill` pass to fetch a contract's current WASM
/// bytecode from Soroban RPC and re-derive its mutability bit.
pub fn contract_code_ledger_key(wasm_hash: [u8; 32]) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash(wasm_hash),
    })
}

/// Build a `LedgerKey::ContractData` for a token's per-holder balance entry —
/// the standard `Vec[Symbol("Balance"), Address(holder)]` **persistent** key
/// (task 0331 RPC seed). `token_strkey` is the token contract (`C…`);
/// `holder_strkey` is a `G…` account or `C…` contract holder. Returns `None`
/// (and warns) on malformed input or a non-contract token so one bad row from
/// the holder-candidate scan can't crash the seed.
pub fn balance_ledger_key(token_strkey: &str, holder_strkey: &str) -> Option<LedgerKey> {
    let contract = sc_address(token_strkey)?;
    if !matches!(contract, ScAddress::Contract(_)) {
        warn!(
            token_strkey,
            "rpc_snapshot: balance-key token must be a C-StrKey; skipping"
        );
        return None;
    }
    let holder = sc_address(holder_strkey)?;
    let key = ScVal::Vec(Some(
        ScVec::try_from(vec![
            ScVal::Symbol(ScSymbol::try_from(b"Balance".to_vec()).ok()?),
            ScVal::Address(holder),
        ])
        .ok()?,
    ));
    Some(LedgerKey::ContractData(LedgerKeyContractData {
        contract,
        key,
        durability: ContractDataDurability::Persistent,
    }))
}

/// Pure decoder: shape a `LedgerEntryData::ContractData` standard balance entry
/// into `(token_strkey, holder_strkey, balance)` (task 0331 RPC seed). Handles
/// BOTH value shapes: a bare `i128` (a type-3 token balance) OR the SAC
/// `BalanceValue { amount, authorized, clawback }` struct (contract-held
/// classic/native — Path X). Returns `None` for non-ContractData variants,
/// non-`Balance` keys, or any other value shape.
///
/// The struct case is decoded by the SAME `xdr_parser::decode_sac_balance_value`
/// the live parser uses (converting the value to the tagged JSON it expects), so
/// seed and live share ONE strict decoder — lock-step by construction, not by a
/// comment. The key-shape check stays on `ScVal` here (the RPC path holds an XDR
/// entry); the bare-`i128` case is a one-liner not worth sharing.
pub fn decode_balance_entry(data: &LedgerEntryData) -> Option<(String, String, i128)> {
    let LedgerEntryData::ContractData(entry) = data else {
        return None;
    };
    if !matches!(entry.contract, ScAddress::Contract(_)) {
        return None;
    }
    let ScVal::Vec(Some(elems)) = &entry.key else {
        return None;
    };
    // Exactly `[Symbol("Balance"), Address(holder)]` — the third `next()` MUST
    // be `None`, else a longer key is some other contract-data shape.
    let mut it = elems.iter();
    let (Some(ScVal::Symbol(sym)), Some(ScVal::Address(holder)), None) =
        (it.next(), it.next(), it.next())
    else {
        return None;
    };
    if sym.0.as_slice() != b"Balance" {
        return None;
    }
    let balance = match &entry.val {
        ScVal::I128(parts) => i128::from(parts),
        ScVal::Map(_) => {
            let json = json!({ "val": xdr_parser::scval_to_typed_json(&entry.val) });
            xdr_parser::decode_sac_balance_value(&json)?.amount
        }
        _ => return None,
    };
    Some((entry.contract.to_string(), holder.to_string(), balance))
}

/// Parse a holder StrKey into an `ScAddress` — `G…` → account, `C…` → contract
/// (34% of type-3 holders are contracts). Warns + `None` on anything else
/// (muxed / garbage) rather than crashing the seed.
fn sc_address(strkey: &str) -> Option<ScAddress> {
    match stellar_strkey::Strkey::from_string(strkey) {
        Ok(stellar_strkey::Strkey::PublicKeyEd25519(pk)) => Some(ScAddress::Account(AccountId(
            PublicKey::PublicKeyTypeEd25519(Uint256(pk.0)),
        ))),
        Ok(stellar_strkey::Strkey::Contract(c)) => Some(ScAddress::Contract(ContractId(Hash(c.0)))),
        _ => {
            warn!(
                strkey,
                "rpc_snapshot: holder StrKey is neither G nor C; skipping"
            );
            None
        }
    }
}

/// Build a `LedgerKey::Trustline` for the given account / asset.
/// Returns `None` on malformed StrKey (account or issuer).
///
/// `asset` carries the trustline's asset shape — native is **never**
/// a valid trustline key (native balances live on the account entry
/// itself), so callers must pass an alphanum asset. The bootstrap
/// step derives trustline keys after decoding `TrustLineEntry`s from
/// a prior account-state batch, so the caller already knows the asset.
///
/// Unused by the current bootstrap path (Phase 1 only snapshots
/// `AccountEntry` and leaves trustlines for the per-ledger ingest +
/// post-bootstrap top-up pass). Kept on the public surface so the
/// follow-up trustline + asset-aggregate task (Phase 3 in the
/// 0214 task body) can reuse it.
#[allow(dead_code)]
pub fn trustline_ledger_key(account_strkey: &str, asset: TrustLineAsset) -> Option<LedgerKey> {
    let pk = match stellar_strkey::ed25519::PublicKey::from_string(account_strkey) {
        Ok(pk) => pk,
        Err(err) => {
            warn!(strkey = account_strkey, %err, "rpc_snapshot: invalid trustline account StrKey; skipping");
            return None;
        }
    };
    let account_id = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk.0)));
    Some(LedgerKey::Trustline(LedgerKeyTrustLine {
        account_id,
        asset,
    }))
}

/// Pure decoder: shape a `LedgerEntryData::Account` into the three
/// fields the CH writer's account staging consumes. Kept separate
/// from the HTTP client so it's unit-testable against fixed XDR
/// fixtures.
///
/// Returns `None` for non-Account variants — caller should match on
/// `data` shape before calling, but the explicit signal makes the
/// pattern less brittle in unit tests.
pub fn decode_account_snapshot(data: &LedgerEntryData) -> Option<AccountSnapshot> {
    let LedgerEntryData::Account(entry) = data else {
        return None;
    };
    let home_domain = String::from_utf8_lossy(entry.home_domain.as_slice())
        .trim_end_matches('\0')
        .to_string();
    Some(AccountSnapshot {
        account_id: entry.account_id.to_string(),
        sequence_number: i64::from(entry.seq_num.clone()),
        balance: entry.balance,
        home_domain: if home_domain.is_empty() {
            None
        } else {
            Some(home_domain)
        },
    })
}

/// Pure decoder: shape a `LedgerEntryData::Trustline` into the
/// (account, asset, balance) tuple the CH writer consumes.
///
/// Skips `TrustLineAsset::PoolShare` (LP positions, not asset
/// balances — owned by task 0162 / `extract_lp_positions`). Skips
/// `TrustLineAsset::Native` defensively even though the network
/// should never emit such a trustline.
///
/// Unused by the Phase 1 bootstrap path (which only snapshots
/// `AccountEntry`); kept on the public surface for the follow-up
/// trustline pass + the unit-test coverage that locks the decoding
/// contract in.
#[allow(dead_code)]
pub fn decode_trustline_snapshot(data: &LedgerEntryData) -> Option<TrustlineSnapshot> {
    let LedgerEntryData::Trustline(entry) = data else {
        return None;
    };
    let asset = match &entry.asset {
        TrustLineAsset::Native => return None,
        TrustLineAsset::PoolShare(_) => return None,
        TrustLineAsset::CreditAlphanum4(a) => TrustlineAsset {
            asset_type: TrustlineAssetType::Alphanum4,
            code: String::from_utf8_lossy(a.asset_code.as_slice())
                .trim_end_matches('\0')
                .to_string(),
            issuer: a.issuer.to_string(),
        },
        TrustLineAsset::CreditAlphanum12(a) => TrustlineAsset {
            asset_type: TrustlineAssetType::Alphanum12,
            code: String::from_utf8_lossy(a.asset_code.as_slice())
                .trim_end_matches('\0')
                .to_string(),
            issuer: a.issuer.to_string(),
        },
    };
    Some(TrustlineSnapshot {
        account_id: entry.account_id.to_string(),
        asset,
        balance: entry.balance,
    })
}

/// Helper: pull every `TrustLineAsset` shape out of an
/// `AccountEntry`'s signers/sub-entries is **not** something the RPC
/// gives us directly. Soroban RPC's `getLedgerEntries` requires
/// trustline keys to be enumerated by the caller; there is no "all
/// trustlines for account X" primitive.
///
/// The bootstrap step gets around this by relying on the parser's
/// observed-asset universe — every `assets` row referenced in the
/// window's event/operation stream is a candidate trustline key for
/// every account in the window. That's an over-shoot but bounded
/// (~few thousand assets × ~100 k accounts at worst → a few
/// hundred-thousand RPC keys, well within the budget). See
/// `crate::bootstrap::collect_trustline_candidates` for the actual
/// pairing logic.
///
/// Unused today — the trustline pairing pass is Phase 3 follow-up.
#[allow(dead_code)]
pub fn rebuild_trustline_asset(
    asset_type: TrustlineAssetType,
    code: &str,
    issuer_strkey: &str,
) -> Option<TrustLineAsset> {
    use stellar_xdr::curr::{AlphaNum4, AlphaNum12, AssetCode4, AssetCode12};
    let issuer_pk = stellar_strkey::ed25519::PublicKey::from_string(issuer_strkey).ok()?;
    let issuer = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(issuer_pk.0)));
    match asset_type {
        TrustlineAssetType::Alphanum4 => {
            let code_bytes = code.as_bytes();
            if code_bytes.len() > 4 {
                return None;
            }
            let mut padded = [0u8; 4];
            padded[..code_bytes.len()].copy_from_slice(code_bytes);
            Some(TrustLineAsset::CreditAlphanum4(AlphaNum4 {
                asset_code: AssetCode4(padded),
                issuer,
            }))
        }
        TrustlineAssetType::Alphanum12 => {
            let code_bytes = code.as_bytes();
            if code_bytes.len() > 12 {
                return None;
            }
            let mut padded = [0u8; 12];
            padded[..code_bytes.len()].copy_from_slice(code_bytes);
            Some(TrustLineAsset::CreditAlphanum12(AlphaNum12 {
                asset_code: AssetCode12(padded),
                issuer,
            }))
        }
    }
}

/// Decoded `AccountEntry` view. Lean: only the fields the CH writer's
/// account staging consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub sequence_number: i64,
    /// Native XLM balance in stroops (1 XLM = 10⁷ stroops).
    pub balance: i64,
    /// `Some(value)` only when non-empty in the AccountEntry.
    pub home_domain: Option<String>,
}

/// Decoded `TrustLineEntry` view. Unused by Phase 1; kept on the
/// public surface for the Phase 3 trustline pass.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustlineSnapshot {
    pub account_id: String,
    pub asset: TrustlineAsset,
    pub balance: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustlineAsset {
    pub asset_type: TrustlineAssetType,
    pub code: String,
    pub issuer: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustlineAssetType {
    Alphanum4,
    Alphanum12,
}

/// JSON-RPC encoding boilerplate. Kept private — callers see the
/// typed API above.
#[derive(Serialize)]
struct JsonRpcRequest<T> {
    jsonrpc: &'static str,
    id: i64,
    method: &'static str,
    params: T,
}

#[derive(Serialize)]
struct GetLedgerEntriesParams {
    keys: Vec<String>,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    #[serde(default)]
    jsonrpc: Option<String>,
    #[serde(default)]
    result: Option<GetLedgerEntriesResult>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct GetLedgerEntriesResult {
    /// Latest sequence the RPC's view reflects. Logged for context;
    /// the bootstrap step uses the window's `start` ledger, not this.
    #[allow(dead_code)]
    #[serde(rename = "latestLedger", default)]
    latest_ledger: Option<i64>,
    #[serde(default)]
    entries: Option<Vec<LedgerEntryEntry>>,
}

#[derive(Deserialize)]
struct LedgerEntryEntry {
    /// Base64-encoded `LedgerKey` XDR.
    key: String,
    /// Base64-encoded `LedgerEntryData` XDR.
    xdr: String,
    #[serde(rename = "lastModifiedLedgerSeq")]
    last_modified_ledger_seq: u32,
}

#[derive(Deserialize, Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
}

/// Typed errors surfacing from the RPC client.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// Transport-level reqwest failure (DNS, connect, body, timeout).
    #[error("rpc http: {0}")]
    Http(reqwest::Error),
    /// Non-2xx HTTP response. Body is preserved verbatim so the operator
    /// can grep the log for whatever Soroban RPC returned.
    #[error("rpc http status {status} body: {body}")]
    HttpStatus { status: u16, body: String },
    /// Encoding `LedgerKey` to base64 XDR failed. Shouldn't happen in
    /// practice — the keys are constructed from typed values — but
    /// surfaced cleanly so a future variant addition doesn't sneak in
    /// silently.
    #[error("rpc encode key: {0}")]
    EncodeKey(String),
    /// JSON or XDR decode of the response body failed.
    #[error("rpc decode: {0}")]
    Decode(String),
    /// JSON-RPC `error` object in the response (e.g. invalid params,
    /// rate-limited, internal error). Caller decides whether to retry.
    #[error("rpc error code={code} message={message}")]
    Rpc { code: i64, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        AccountEntry, AccountEntryExt, AlphaNum4, AssetCode4, Int64, SequenceNumber, String32,
        Thresholds, TrustLineEntry, TrustLineEntryExt, VecM,
    };

    fn make_account_id(byte: u8) -> AccountId {
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([byte; 32])))
    }

    fn make_account_entry(
        account_id: AccountId,
        balance: i64,
        seq: i64,
        home_domain: &str,
    ) -> LedgerEntryData {
        LedgerEntryData::Account(AccountEntry {
            account_id,
            balance: Int64::from(balance),
            seq_num: SequenceNumber(seq),
            num_sub_entries: 0,
            inflation_dest: None,
            flags: 0,
            home_domain: String32::try_from(home_domain.as_bytes().to_vec()).unwrap(),
            thresholds: Thresholds([1, 0, 0, 0]),
            signers: VecM::default(),
            ext: AccountEntryExt::V0,
        })
    }

    #[test]
    fn decode_account_snapshot_extracts_fields() {
        let acct = make_account_id(0xAA);
        let data = make_account_entry(acct.clone(), 1_000_000, 42, "example.com");
        let snap = decode_account_snapshot(&data).expect("Account variant must decode");
        assert_eq!(snap.account_id, acct.to_string());
        assert_eq!(snap.balance, 1_000_000);
        assert_eq!(snap.sequence_number, 42);
        assert_eq!(snap.home_domain.as_deref(), Some("example.com"));
    }

    #[test]
    fn decode_account_snapshot_empty_home_domain_is_none() {
        let acct = make_account_id(0xBB);
        let data = make_account_entry(acct, 0, 0, "");
        let snap = decode_account_snapshot(&data).unwrap();
        assert_eq!(snap.home_domain, None);
    }

    #[test]
    fn decode_account_snapshot_rejects_non_account_variant() {
        let acct = make_account_id(0xCC);
        let asset = TrustLineAsset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"USDC"),
            issuer: make_account_id(0xDD),
        });
        let data = LedgerEntryData::Trustline(TrustLineEntry {
            account_id: acct,
            asset,
            balance: 0,
            limit: 0,
            flags: 0,
            ext: TrustLineEntryExt::V0,
        });
        assert!(decode_account_snapshot(&data).is_none());
    }

    #[test]
    fn decode_trustline_snapshot_alphanum4() {
        let owner = make_account_id(0xEE);
        let issuer = make_account_id(0xFF);
        let asset = TrustLineAsset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"USDC"),
            issuer: issuer.clone(),
        });
        let data = LedgerEntryData::Trustline(TrustLineEntry {
            account_id: owner.clone(),
            asset,
            balance: 12_345_678,
            limit: 0,
            flags: 1,
            ext: TrustLineEntryExt::V0,
        });
        let snap = decode_trustline_snapshot(&data).expect("trustline must decode");
        assert_eq!(snap.account_id, owner.to_string());
        assert_eq!(snap.asset.asset_type, TrustlineAssetType::Alphanum4);
        assert_eq!(snap.asset.code, "USDC");
        assert_eq!(snap.asset.issuer, issuer.to_string());
        assert_eq!(snap.balance, 12_345_678);
    }

    #[test]
    fn decode_trustline_snapshot_skips_pool_share_and_native() {
        let owner = make_account_id(0x11);
        let native = LedgerEntryData::Trustline(TrustLineEntry {
            account_id: owner.clone(),
            asset: TrustLineAsset::Native,
            balance: 1,
            limit: 0,
            flags: 0,
            ext: TrustLineEntryExt::V0,
        });
        assert!(decode_trustline_snapshot(&native).is_none());

        use stellar_xdr::curr::{Hash, PoolId};
        let pool_share = LedgerEntryData::Trustline(TrustLineEntry {
            account_id: owner,
            asset: TrustLineAsset::PoolShare(PoolId(Hash([0u8; 32]))),
            balance: 1,
            limit: 0,
            flags: 0,
            ext: TrustLineEntryExt::V0,
        });
        assert!(decode_trustline_snapshot(&pool_share).is_none());
    }

    #[test]
    fn account_ledger_key_round_trips_strkey() {
        // Mainnet pin: account `GARDNV3Q7…` from audit doc §E06.
        let strkey = "GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55";
        let key = account_ledger_key(strkey).expect("valid G-StrKey must decode");
        let LedgerKey::Account(k) = key else {
            panic!("expected LedgerKey::Account variant");
        };
        assert_eq!(k.account_id.to_string(), strkey);
    }

    #[test]
    fn account_ledger_key_rejects_bad_strkey() {
        assert!(account_ledger_key("not-a-strkey").is_none());
        // Wrong prefix (M-StrKey for muxed accounts — rejected, RPC
        // doesn't accept muxed account keys here).
        assert!(account_ledger_key("MAAAAAAAAAAAAAA").is_none());
    }

    #[test]
    fn rebuild_trustline_asset_alphanum4_round_trip() {
        let issuer_strkey = "GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55";
        let asset = rebuild_trustline_asset(TrustlineAssetType::Alphanum4, "USDC", issuer_strkey)
            .expect("alphanum4 round trip");
        let TrustLineAsset::CreditAlphanum4(an) = asset else {
            panic!("expected CreditAlphanum4 variant");
        };
        assert_eq!(&an.asset_code.0, b"USDC");
        assert_eq!(an.issuer.to_string(), issuer_strkey);
    }

    #[test]
    fn rebuild_trustline_asset_rejects_oversize_code() {
        // Use a valid issuer StrKey so the test specifically exercises
        // the oversize-code branch — an invalid issuer would return
        // None regardless of the code length and mask the regression
        // we want to catch.
        let issuer = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
        assert!(
            rebuild_trustline_asset(TrustlineAssetType::Alphanum4, "TOOLONG", issuer).is_none()
        );
    }

    // --- task 0331: per-holder token balance key + entry decode (RPC seed) ---

    use stellar_xdr::curr::{
        ContractDataDurability, ContractDataEntry, ContractId, ExtensionPoint, Int128Parts,
        ScAddress, ScMap, ScMapEntry, ScSymbol, ScVal, ScVec,
    };

    fn balance_entry(token: [u8; 32], holder: ScAddress, val: ScVal) -> LedgerEntryData {
        LedgerEntryData::ContractData(ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: ScAddress::Contract(ContractId(Hash(token))),
            key: ScVal::Vec(Some(
                ScVec::try_from(vec![
                    ScVal::Symbol(ScSymbol::try_from(b"Balance".to_vec()).unwrap()),
                    ScVal::Address(holder),
                ])
                .unwrap(),
            )),
            durability: ContractDataDurability::Persistent,
            val,
        })
    }

    #[test]
    fn balance_ledger_key_builds_persistent_balance_key() {
        let token = stellar_strkey::Contract([0x11u8; 32]).to_string();
        let holder = stellar_strkey::ed25519::PublicKey([0x22u8; 32]).to_string();
        let key = balance_ledger_key(&token, &holder).expect("valid C token + G holder");
        let LedgerKey::ContractData(k) = key else {
            panic!("expected LedgerKey::ContractData variant");
        };
        assert_eq!(k.contract.to_string(), token.as_str());
        assert_eq!(k.durability, ContractDataDurability::Persistent);
        let ScVal::Vec(Some(elems)) = &k.key else {
            panic!("balance key must be a vec");
        };
        let parts: Vec<&ScVal> = elems.iter().collect();
        assert_eq!(parts.len(), 2, "key is [Symbol(Balance), Address(holder)]");
        let ScVal::Symbol(sym) = parts[0] else {
            panic!("first elem must be a symbol");
        };
        assert_eq!(sym.0.as_slice(), b"Balance");
        let ScVal::Address(addr) = parts[1] else {
            panic!("second elem must be an address");
        };
        assert_eq!(
            addr.to_string(),
            holder.as_str(),
            "holder address round-trips"
        );
    }

    #[test]
    fn balance_ledger_key_rejects_account_token_and_bad_holder() {
        // The token must be a C-StrKey (a contract), never a G-account.
        let g = stellar_strkey::ed25519::PublicKey([0x33u8; 32]).to_string();
        assert!(balance_ledger_key(&g, &g).is_none());
        // Malformed holder StrKey is skipped, not panicked.
        let c = stellar_strkey::Contract([0x44u8; 32]).to_string();
        assert!(balance_ledger_key(&c, "not-a-strkey").is_none());
    }

    /// Real mainnet end-to-end (RPC `getLedgerEntries`, 2026-07-01): the actual
    /// `Balance(GAWOKP6N…)` ContractData entry for token `CCSNFZ5R…` at ledger
    /// 63268948, decoded straight from the on-chain XDR. The decoded value equals
    /// the independent `stellar contract invoke … balance` read (10000040000000)
    /// — NON-circular: these bytes are the ledger's, not the test's.
    #[test]
    fn decode_balance_entry_real_mainnet() {
        let entry_b64 = "AAAABgAAAAAAAAABpNLnsQaIecmK0DuR3iIEA4DUoHpK2z+hSQS0L4ntArUAAAAQAAAAAQAAAAIAAAAPAAAAB0JhbGFuY2UAAAAAEgAAAAAAAAAALOU/zUgs2L4DJx225wMqTkYuiH78AX+HaE65g2akcB4AAAABAAAACgAAAAAAAAAAAAAJGFDU+gA=";
        let bytes = BASE64.decode(entry_b64.as_bytes()).unwrap();
        let data = LedgerEntryData::from_xdr(&bytes, Limits::none()).unwrap();
        let (token, holder, balance) =
            decode_balance_entry(&data).expect("standard bare-i128 balance entry");
        assert_eq!(
            token,
            "CCSNFZ5RA2EHTSMK2A5ZDXRCAQBYBVFAPJFNWP5BJECLIL4J5UBLLUQG"
        );
        assert_eq!(
            holder,
            "GAWOKP6NJAWNRPQDE4O3NZYDFJHEMLUIP36AC74HNBHLTA3GURYB4PYJ"
        );
        assert_eq!(balance, 10_000_040_000_000);
    }

    /// Real mainnet (RPC `getLedgerEntries`, 2026-07-01): the ACTUAL SAC
    /// `BalanceValue` struct entry for the pool `CATUJXDU…` holding native XLM in
    /// the XLM SAC — the contract-held classic/native shape the seed must now
    /// decode. The amount comes from the ledger bytes and equals the independent
    /// `balance()`/`get_reserves()` read (11_635_129_310_963) — NON-circular.
    #[test]
    fn decode_balance_entry_sac_struct_real_mainnet() {
        let entry_b64 = "AAAABgAAAAAAAAABJbT82FmuwvpjSEOMSJs8PBDJi20hvk/TyzDLaJU++XcAAAAQAAAAAQAAAAIAAAAPAAAAB0JhbGFuY2UAAAAAEgAAAAEnRNx0d+UpTAqVK9xT0ZTDwPQVQeA669CYruDz0/SWywAAAAEAAAARAAAAAQAAAAMAAAAPAAAABmFtb3VudAAAAAAACgAAAAAAAAAAAAAKlQO/3vMAAAAPAAAACmF1dGhvcml6ZWQAAAAAAAAAAAABAAAADwAAAAhjbGF3YmFjawAAAAAAAAAA";
        let bytes = BASE64.decode(entry_b64.as_bytes()).unwrap();
        let data = LedgerEntryData::from_xdr(&bytes, Limits::none()).unwrap();
        let (token, holder, balance) =
            decode_balance_entry(&data).expect("SAC BalanceValue struct balance entry");
        assert_eq!(
            token,
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
        );
        assert_eq!(
            holder,
            "CATUJXDUO7SSSTAKSUV5YU6RSTB4B5AVIHQDV26QTCXOB46T6SLMWNMY"
        );
        assert_eq!(balance, 11_635_129_310_963);
    }

    /// E2E through the WHOLE write pipeline with the REAL mainnet native-XLM SAC
    /// balance entry (pool CATUJXDU holding XLM): decode → `build_balance_rows` with
    /// the SAC→classic map. Proves the contract-held balance keys OFF the SAC
    /// surrogate ONTO the native asset id (ADR 0051 / the Path-X replacement) — the
    /// SAME id an account's native balance uses, so contract-held + trustline supply
    /// land on ONE row. NON-circular (real ledger bytes).
    #[test]
    fn sac_balance_rekeys_to_native_asset_id_real_mainnet() {
        use db_clickhouse::persist::ids;
        use db_clickhouse::persist::stage::build_balance_rows;
        use std::collections::HashMap;
        use xdr_parser::ExtractedSorobanBalance;

        let entry_b64 = "AAAABgAAAAAAAAABJbT82FmuwvpjSEOMSJs8PBDJi20hvk/TyzDLaJU++XcAAAAQAAAAAQAAAAIAAAAPAAAAB0JhbGFuY2UAAAAAEgAAAAEnRNx0d+UpTAqVK9xT0ZTDwPQVQeA669CYruDz0/SWywAAAAEAAAARAAAAAQAAAAMAAAAPAAAABmFtb3VudAAAAAAACgAAAAAAAAAAAAAKlQO/3vMAAAAPAAAACmF1dGhvcml6ZWQAAAAAAAAAAAABAAAADwAAAAhjbGF3YmFjawAAAAAAAAAA";
        let bytes = BASE64.decode(entry_b64.as_bytes()).unwrap();
        let data = LedgerEntryData::from_xdr(&bytes, Limits::none()).unwrap();
        let (token, holder, balance) = decode_balance_entry(&data).expect("decodes");
        let bal = ExtractedSorobanBalance {
            contract_id: token.clone(),
            holder,
            balance,
            ledger: 63_280_279,
        };
        let sac_surrogate = ids::contract_id(&token);

        // Without the asset_sac map, the contract-held balance would key by the SAC
        // surrogate — which has no `assets` row (it would orphan).
        let orphan = build_balance_rows(std::slice::from_ref(&bal), &HashMap::new());
        assert_eq!(
            orphan[0].asset_id, sac_surrogate,
            "no map → SAC surrogate (would orphan)"
        );

        // With the asset_sac map (native XLM SAC surrogate → native asset id), the
        // balance keys onto the native asset id — the SAME id an account's native
        // balance uses, so contract-held + trustline supply land on one row.
        let native_id = ids::asset_id(0, "", 0, 0);
        let map = HashMap::from([(sac_surrogate, native_id)]);
        let rows = build_balance_rows(std::slice::from_ref(&bal), &map);
        assert_eq!(rows[0].asset_id, native_id, "with map → native asset id");
        assert_eq!(rows[0].amount, 11_635_129_310_963, "amount preserved");
    }

    #[test]
    fn decode_balance_entry_extracts_contract_holder_balance() {
        let holder = ScAddress::Account(make_account_id(0x55));
        let data = balance_entry(
            [0x11u8; 32],
            holder.clone(),
            ScVal::I128(Int128Parts {
                hi: 0,
                lo: 1_000_000,
            }),
        );
        let (token_sk, holder_sk, bal) =
            decode_balance_entry(&data).expect("standard balance entry must decode");
        assert_eq!(
            token_sk,
            ScAddress::Contract(ContractId(Hash([0x11u8; 32]))).to_string()
        );
        assert_eq!(holder_sk, holder.to_string());
        assert_eq!(bal, 1_000_000);
    }

    #[test]
    fn decode_balance_entry_rejects_wrong_shapes() {
        // non-ContractData variant
        assert!(decode_balance_entry(&make_account_entry(make_account_id(1), 0, 0, "")).is_none());
        // wrong symbol (not "Balance")
        let holder = ScAddress::Account(make_account_id(0x66));
        let wrong_sym = LedgerEntryData::ContractData(ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: ScAddress::Contract(ContractId(Hash([0x11u8; 32]))),
            key: ScVal::Vec(Some(
                ScVec::try_from(vec![
                    ScVal::Symbol(ScSymbol::try_from(b"State".to_vec()).unwrap()),
                    ScVal::Address(holder.clone()),
                ])
                .unwrap(),
            )),
            durability: ContractDataDurability::Persistent,
            val: ScVal::I128(Int128Parts { hi: 0, lo: 5 }),
        });
        assert!(decode_balance_entry(&wrong_sym).is_none());
        // an unsupported value shape (neither bare i128 nor the SAC struct map)
        let other = balance_entry([0x11u8; 32], holder.clone(), ScVal::U64(5));
        assert!(decode_balance_entry(&other).is_none());
        // a Map that is NOT the exact {amount,authorized,clawback} SAC struct
        // (only `amount`) → None: strict, never partial-decode a foreign map.
        let foreign_map = balance_entry(
            [0x11u8; 32],
            holder,
            ScVal::Map(Some(
                ScMap::try_from(vec![ScMapEntry {
                    key: ScVal::Symbol(ScSymbol::try_from(b"amount".to_vec()).unwrap()),
                    val: ScVal::I128(Int128Parts { hi: 0, lo: 5 }),
                }])
                .unwrap(),
            )),
        );
        assert!(decode_balance_entry(&foreign_map).is_none());
    }

    // --- task 0331: instance `TotalSupply` key (authoritative supply) ---
}
