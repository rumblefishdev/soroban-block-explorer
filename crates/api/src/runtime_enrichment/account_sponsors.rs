//! Who pays each sponsored reserve of one account (CAP-33, issue #454), read
//! live from RPC: `getLedgerEntries` for the account and its trustlines, and
//! the `sponsoring_id` each entry carries in its extension (`extXdr`).
//!
//! Live rather than indexed: the answer sits on the account's own entries, a
//! few RPC calls hold all of them, and it is never stale — a closed account
//! simply returns no entry.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use stellar_xdr::{
    AccountEntryExt, AccountEntryExtensionV1Ext, AccountId, AlphaNum4, AlphaNum12, AssetCode4,
    AssetCode12, LedgerEntryData, LedgerEntryExt, LedgerKey, LedgerKeyAccount, LedgerKeyTrustLine,
    Limits, ReadXdr, TrustLineAsset, WriteXdr,
};

use super::rpc_pool::{RpcFailure, RpcPool};

/// `getLedgerEntries` accepts at most 200 keys a call.
const KEYS_PER_CALL: usize = 200;

/// Five calls at most: the account plus 999 trustlines. An account with more
/// lists the first 999 and says so through the reserve count.
const MAX_TRUSTLINES: usize = 999;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("invalid key: {0}")]
    Key(String),
    #[error("XDR encode/decode: {0}")]
    Xdr(String),
    #[error(transparent)]
    Rpc(#[from] RpcFailure),
}

/// One sponsored entry of the account.
#[derive(Debug, PartialEq)]
pub struct SponsoredEntry {
    /// `account` | `trustline` | `signer`.
    pub kind: &'static str,
    /// `CODE-ISSUER` for a trustline.
    pub asset: Option<String>,
    /// The signer's key for a signer.
    pub signer: Option<String>,
    /// Reserves the entry costs: 2 for the account, 1 otherwise.
    pub reserves: u32,
    /// The account paying them.
    pub sponsor: String,
}

#[derive(Debug, PartialEq)]
pub struct AccountSponsors {
    /// `num_sponsored` from the account entry, as the chain holds it now.
    pub num_sponsored: u32,
    pub entries: Vec<SponsoredEntry>,
}

#[derive(Clone)]
pub struct AccountSponsorsFetcher {
    rpc: RpcPool,
}

impl AccountSponsorsFetcher {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            rpc: RpcPool::new()?,
        })
    }

    /// A fetcher over fixed RPC endpoints — for tests that stand up their own.
    #[cfg(test)]
    pub(crate) fn with_urls(rpc_urls: Vec<String>) -> Self {
        Self {
            rpc: RpcPool::with_urls(rpc_urls),
        }
    }

    /// The sponsored entries of `account` among itself and the given classic
    /// trustlines (`code`, `issuer`). `None` when the account has no entry on
    /// the ledger.
    pub async fn fetch(
        &self,
        account: &str,
        trustlines: &[(String, String)],
    ) -> Result<Option<AccountSponsors>, FetchError> {
        let account_id: AccountId = account
            .parse()
            .map_err(|_| FetchError::Key(account.into()))?;
        let mut keys = vec![LedgerKey::Account(LedgerKeyAccount {
            account_id: account_id.clone(),
        })];
        for (code, issuer) in trustlines.iter().take(MAX_TRUSTLINES) {
            // A stored code that is no valid asset code names no trustline we
            // could ask for; it drops out of the list, not the whole answer.
            let Ok(asset) = trustline_asset(code, issuer) else {
                continue;
            };
            keys.push(LedgerKey::Trustline(LedgerKeyTrustLine {
                account_id: account_id.clone(),
                asset,
            }));
        }
        let mut keys_b64 = Vec::with_capacity(keys.len());
        for key in &keys {
            let bytes = key
                .to_xdr(Limits::none())
                .map_err(|e| FetchError::Xdr(e.to_string()))?;
            keys_b64.push(BASE64.encode(bytes));
        }
        let mut chunks = keys_b64.chunks(KEYS_PER_CALL);
        let Some(first) = chunks.next() else {
            return Ok(None);
        };
        // The account rides in the first call; without it there is nothing to
        // list, and no reason to ask for the rest.
        let mut decoded = decode_entries(&self.rpc.get_ledger_entries(first.to_vec()).await?)?;
        if !decoded
            .iter()
            .any(|(data, _)| matches!(data, LedgerEntryData::Account(_)))
        {
            return Ok(None);
        }
        for chunk in chunks {
            // A later call failing leaves the list shorter, not the answer
            // gone: the page's "N of M reserves" shows what is missing.
            match self.rpc.get_ledger_entries(chunk.to_vec()).await {
                Ok(more) => decoded.extend(decode_entries(&more)?),
                Err(e) => {
                    tracing::warn!(account, error = %e, "sponsors: a trustline batch failed; listing what was read");
                    break;
                }
            }
        }
        Ok(sponsors(decoded))
    }
}

/// Each entry's data and sponsor, as RPC returned them.
fn decode_entries(
    entries: &[serde_json::Value],
) -> Result<Vec<(LedgerEntryData, Option<String>)>, FetchError> {
    let mut decoded = Vec::with_capacity(entries.len());
    for e in entries {
        let Some(xdr) = e["xdr"].as_str() else {
            continue;
        };
        decoded.push((
            decode::<LedgerEntryData>(xdr)?,
            sponsor_of(e["extXdr"].as_str())?,
        ));
    }
    Ok(decoded)
}

fn trustline_asset(code: &str, issuer: &str) -> Result<TrustLineAsset, FetchError> {
    let issuer: AccountId = issuer.parse().map_err(|_| FetchError::Key(issuer.into()))?;
    if code.is_empty() || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(FetchError::Key(code.into()));
    }
    if code.len() <= 4 {
        let mut bytes = [0u8; 4];
        bytes[..code.len()].copy_from_slice(code.as_bytes());
        Ok(TrustLineAsset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(bytes),
            issuer,
        }))
    } else if code.len() <= 12 {
        let mut bytes = [0u8; 12];
        bytes[..code.len()].copy_from_slice(code.as_bytes());
        Ok(TrustLineAsset::CreditAlphanum12(AlphaNum12 {
            asset_code: AssetCode12(bytes),
            issuer,
        }))
    } else {
        Err(FetchError::Key(code.into()))
    }
}

fn decode<T: ReadXdr>(b64: &str) -> Result<T, FetchError> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|e| FetchError::Xdr(format!("base64: {e}")))?;
    T::from_xdr(bytes, Limits::none()).map_err(|e| FetchError::Xdr(e.to_string()))
}

/// The entry's sponsor, from its extension. No extension: nobody.
fn sponsor_of(ext_xdr: Option<&str>) -> Result<Option<String>, FetchError> {
    let Some(b64) = ext_xdr else { return Ok(None) };
    match decode::<LedgerEntryExt>(b64)? {
        LedgerEntryExt::V1(v1) => Ok(v1.sponsoring_id.0.map(|id| id.to_string())),
        LedgerEntryExt::V0 => Ok(None),
    }
}

/// Fold the decoded entries into the sponsored ones. `None` when the account
/// entry itself is absent.
fn sponsors(entries: Vec<(LedgerEntryData, Option<String>)>) -> Option<AccountSponsors> {
    let mut num_sponsored = None;
    let mut out = Vec::new();
    for (data, sponsor) in entries {
        match data {
            LedgerEntryData::Account(a) => {
                let mut signer_sponsors = Vec::new();
                num_sponsored = Some(0);
                if let AccountEntryExt::V1(v1) = &a.ext
                    && let AccountEntryExtensionV1Ext::V2(v2) = &v1.ext
                {
                    num_sponsored = Some(v2.num_sponsored);
                    signer_sponsors = v2.signer_sponsoring_i_ds.to_vec();
                }
                if let Some(sponsor) = sponsor {
                    out.push(SponsoredEntry {
                        kind: "account",
                        asset: None,
                        signer: None,
                        reserves: 2,
                        sponsor,
                    });
                }
                // `signer_sponsoring_i_ds` runs parallel to `signers`.
                for (signer, sponsor) in a.signers.iter().zip(signer_sponsors) {
                    if let Some(id) = sponsor.0 {
                        out.push(SponsoredEntry {
                            kind: "signer",
                            asset: None,
                            signer: Some(signer.key.to_string()),
                            reserves: 1,
                            sponsor: id.to_string(),
                        });
                    }
                }
            }
            LedgerEntryData::Trustline(t) => {
                let Some(sponsor) = sponsor else { continue };
                let (code, issuer) = match &t.asset {
                    TrustLineAsset::CreditAlphanum4(a) => (a.asset_code.as_slice(), &a.issuer),
                    TrustLineAsset::CreditAlphanum12(a) => (a.asset_code.as_slice(), &a.issuer),
                    TrustLineAsset::Native | TrustLineAsset::PoolShare(_) => continue,
                };
                let asset = format!("{}-{issuer}", xdr_parser::asset_code::asset_code_str(code));
                out.push(SponsoredEntry {
                    kind: "trustline",
                    asset: Some(asset),
                    signer: None,
                    reserves: 1,
                    sponsor,
                });
            }
            _ => {}
        }
    }
    num_sponsored.map(|num_sponsored| AccountSponsors {
        num_sponsored,
        entries: out,
    })
}

#[cfg(test)]
mod tests;
