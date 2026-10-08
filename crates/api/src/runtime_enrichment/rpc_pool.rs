//! The Soroban RPC pool the runtime fetchers share: `getLedgerEntries` with
//! failover across `SOROBAN_RPC_URLS` (comma-sep) → `SOROBAN_RPC_URL` → the SDF
//! mainnet default.

use std::sync::Arc;
use std::time::Duration;

/// SDF public mainnet RPC — the single default when no `SOROBAN_RPC_URLS` /
/// `SOROBAN_RPC_URL` env is set.
const DEFAULT_SOROBAN_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

#[derive(Debug, thiserror::Error)]
pub enum RpcFailure {
    #[error("all RPC endpoints failed; last: {0}")]
    Unreachable(String),
    #[error("RPC returned an error object: {0}")]
    ErrorObject(String),
}

/// A pooled RPC client. Cheaply cloneable.
#[derive(Clone)]
pub struct RpcPool {
    client: reqwest::Client,
    rpc_urls: Arc<Vec<String>>,
}

impl RpcPool {
    pub fn new() -> Result<Self, reqwest::Error> {
        let rpc_urls = std::env::var("SOROBAN_RPC_URLS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .or_else(|| std::env::var("SOROBAN_RPC_URL").ok().map(|u| vec![u]))
            .unwrap_or_else(|| vec![DEFAULT_SOROBAN_RPC_URL.to_owned()]);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("sorobanscan-api")
            .build()?;
        Ok(Self {
            client,
            rpc_urls: Arc::new(rpc_urls),
        })
    }

    /// A pool over fixed endpoints — for tests that stand up their own.
    #[cfg(test)]
    pub(crate) fn with_urls(rpc_urls: Vec<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            rpc_urls: Arc::new(rpc_urls),
        }
    }

    /// The entries `getLedgerEntries` returns for `keys` (base64 `LedgerKey`
    /// XDR), as the RPC sent them. Empty when every endpoint answered and none
    /// holds an entry.
    pub async fn get_ledger_entries(
        &self,
        keys: Vec<String>,
    ) -> Result<Vec<serde_json::Value>, RpcFailure> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLedgerEntries",
            "params": { "keys": keys },
        });

        let mut last_err = String::from("no endpoints configured");
        let mut empty_answers = 0usize;
        for url in self.rpc_urls.iter() {
            let resp = match self.client.post(url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_err = e.to_string();
                    continue;
                }
            };
            if !resp.status().is_success() {
                last_err = format!("{url}: HTTP {}", resp.status());
                continue;
            }
            let value: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    last_err = e.to_string();
                    continue;
                }
            };
            // `get("error")` also matches servers that always send the key
            // with a null value — only a non-null payload is a real error.
            if !value["error"].is_null() {
                return Err(RpcFailure::ErrorObject(value["error"].to_string()));
            }
            let entries = value["result"]["entries"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !entries.iter().any(|e| e["xdr"].is_string()) {
                // An empty result means "no live entry" — but a lagging or
                // pruned node reports the same thing for an entry that does
                // exist. Ask the rest of the pool before believing it; the
                // mainnet sweep found 0 genuinely archived binaries, so a
                // single empty answer is more likely a bad node than truth.
                empty_answers += 1;
                last_err = format!("{url}: no entries for these keys");
                continue;
            }
            return Ok(entries);
        }
        // Only conclude "not live" when every endpoint answered cleanly and
        // agreed the entry is gone; a mixed bag of failures is an error.
        if empty_answers == self.rpc_urls.len() {
            return Ok(Vec::new());
        }
        Err(RpcFailure::Unreachable(last_err))
    }
}
