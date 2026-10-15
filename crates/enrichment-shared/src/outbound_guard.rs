//! Outbound-host guard for fetches whose URL comes from chain data (task 0644).
//!
//! An issuer's `home_domain` and an NFT contract's `token_uri` are written by
//! whoever controls the account or contract. A name that resolves to
//! `127.0.0.1`, to `169.254.169.254` (the instance metadata service) or to a
//! private range would otherwise make our Lambda fetch an internal address on
//! the writer's behalf. This resolver looks the name up once, refuses the host
//! when any of its addresses is not public, and hands the checked addresses to
//! the connection — nothing looks the name up again between check and connect.
//!
//! "Not public" = listed in the IANA IPv4 / IPv6 Special-Purpose Address Space
//! registries (loopback, RFC 1918, link-local, CGNAT, unique-local, NAT64,
//! 6to4, documentation, reserved …; the `iprfc` crate's `RFC6890` aggregate is
//! a pinned snapshot of both registries), or multicast, which IANA keeps in a
//! registry of its own. An IPv4-mapped IPv6 address is judged as the IPv4
//! address it carries.
//!
//! IP-literal hosts never reach a resolver — the connector dials them
//! directly — so every caller refuses IP literals itself
//! (`sep1::validate_host`, `nft_token_uri::validate_uri`, and the shared
//! redirect policy for every redirect target).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use tracing::warn;

/// The host resolved to an address this guard never connects to.
#[derive(Debug, thiserror::Error)]
#[error("refused {host}: it resolves to {addr}, which is not a public address")]
pub struct HostRefused {
    pub host: String,
    pub addr: IpAddr,
}

/// Resolver for `reqwest::ClientBuilder::dns_resolver`.
pub fn resolver() -> Arc<PublicOnlyResolver> {
    Arc::new(PublicOnlyResolver)
}

pub struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            // Port 0: the connector sets the URL's port on every address.
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            check_addrs(&host, &addrs)?;
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Refuse the whole host when any one address is not public: the answer comes
/// from DNS the writer controls, and the connector may try any of them.
pub fn check_addrs(host: &str, addrs: &[SocketAddr]) -> Result<(), HostRefused> {
    match addrs.iter().map(SocketAddr::ip).find(|ip| !is_public(*ip)) {
        Some(addr) => {
            warn!(%host, %addr, "outbound fetch refused: host resolves to a non-public address");
            Err(HostRefused {
                host: host.to_owned(),
                addr,
            })
        }
        None => Ok(()),
    }
}

pub fn is_public(ip: IpAddr) -> bool {
    let ip = ip.to_canonical();
    !ip.is_multicast() && !iprfc::RFC6890.contains(&ip)
}

#[cfg(test)]
mod tests;
