//! The Soroban RPC pool of the network a program serves.
//!
//! Every program that asks a Soroban RPC — the API, the enrichment worker,
//! the enrichment backfill CLI — reads its pool from `SOROBAN_RPC_URLS`
//! (comma-separated, tried round-robin with failover). There is no default:
//! an RPC answers for one network only, and a mainnet list in code sent the
//! testnet worker to mainnet, which knew none of its NFTs (lore-0553). The
//! deployed lists live in `infra/envs/<env>.json` as `sorobanRpcUrls`.

/// Name of the env var that carries the pool.
const RPC_URLS_ENV: &str = "SOROBAN_RPC_URLS";

/// The pool from `SOROBAN_RPC_URLS`, or an error naming the variable when it
/// is unset or holds no URL.
pub fn rpc_urls_from_env() -> Result<Vec<String>, String> {
    parse_rpc_urls(std::env::var(RPC_URLS_ENV).ok().as_deref())
}

fn parse_rpc_urls(raw: Option<&str>) -> Result<Vec<String>, String> {
    let urls: Vec<String> = raw
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if urls.is_empty() {
        return Err(format!(
            "{RPC_URLS_ENV} is unset or empty: give the comma-separated Soroban RPC \
             endpoints of the network this program serves (the deployed lists \
             are `sorobanRpcUrls` in infra/envs/<env>.json)"
        ));
    }
    Ok(urls)
}

#[cfg(test)]
mod tests;
