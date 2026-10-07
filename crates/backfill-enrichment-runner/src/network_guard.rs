//! Start-up guard of the RPC-backed subcommands: the Soroban RPC pool,
//! `STELLAR_NETWORK_PASSPHRASE` and the ClickHouse database must all name
//! one network (lore-0553).
//!
//! An RPC answers for its own network only. A testnet pool with
//! `CLICKHOUSE_DATABASE` left unset would ask testnet about mainnet's NFTs,
//! find none and write empty sentinels into mainnet's `default` database —
//! and `--force-retry` would overwrite real rows. So before any write, every
//! URL of the pool is asked for its network, and the run stops unless that
//! network, the passphrase and the database agree.

use std::time::Duration;

use enrichment_shared::soroban_rpc::rpc_urls_from_env;

/// Ask every RPC of `SOROBAN_RPC_URLS` for its network and refuse unless it
/// is `STELLAR_NETWORK_PASSPHRASE`, the network of the target database.
pub async fn check_configured_network() -> Result<(), String> {
    let passphrase = std::env::var("STELLAR_NETWORK_PASSPHRASE").unwrap_or_default();
    if passphrase.trim().is_empty() {
        return Err(
            "STELLAR_NETWORK_PASSPHRASE is not set: give the passphrase of \
                    the network the RPC pool and the database belong to"
                .to_string(),
        );
    }
    let database = db_clickhouse::database_from_env();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("cannot build the HTTP client: {e}"))?;
    for url in rpc_urls_from_env()? {
        let rpc_passphrase = rpc_network_passphrase(&http, &url).await?;
        check_network(&url, &rpc_passphrase, passphrase.trim(), &database)?;
    }
    Ok(())
}

/// The decision, without the network: the RPC at `rpc_url` reported
/// `rpc_passphrase`; the process is configured for `passphrase` and writes
/// into `database`.
pub fn check_network(
    rpc_url: &str,
    rpc_passphrase: &str,
    passphrase: &str,
    database: &str,
) -> Result<(), String> {
    if rpc_passphrase != passphrase {
        return Err(format!(
            "RPC {rpc_url} serves the network \"{rpc_passphrase}\", \
             but STELLAR_NETWORK_PASSPHRASE is \"{passphrase}\""
        ));
    }
    match db_clickhouse::database_network_passphrase(database) {
        Some(expected) if expected == passphrase => Ok(()),
        Some(expected) => Err(format!(
            "database `{database}` holds the network \"{expected}\", but the RPC \
             and STELLAR_NETWORK_PASSPHRASE are \"{passphrase}\" \
             (CLICKHOUSE_DATABASE unset means `default`, mainnet)"
        )),
        None => Err(format!(
            "database `{database}` belongs to no known network \
             (`default` is mainnet, `testnet` is testnet)"
        )),
    }
}

/// JSON-RPC `getNetwork` → `result.passphrase`.
async fn rpc_network_passphrase(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "getNetwork" });
    let response = http
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("getNetwork on {url} failed: {e}"))?;
    let reply: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("getNetwork on {url} returned no JSON: {e}"))?;
    match reply["result"]["passphrase"].as_str() {
        Some(passphrase) => Ok(passphrase.to_string()),
        None => Err(format!(
            "getNetwork on {url} returned no passphrase: {reply}"
        )),
    }
}

#[cfg(test)]
mod tests;
