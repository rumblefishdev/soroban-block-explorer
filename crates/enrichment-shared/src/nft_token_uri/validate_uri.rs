//! `token_uri` value safety check, run before any RPC-returned URI is fetched.

use std::net::IpAddr;

use super::errors::NftTokenUriError;

/// URI safety check: only `https://` (RFC1035 host, no IP literal /
/// userinfo) and `ipfs://` (non-empty CID) pass.
pub(super) fn validate_uri(uri: &str) -> Result<(), NftTokenUriError> {
    let uri = uri.trim();
    let bad = || NftTokenUriError::MalformedUri {
        uri: uri.to_owned(),
    };
    if uri.is_empty() {
        return Err(bad());
    }
    let host = if let Some(rest) = uri.strip_prefix("https://") {
        rest
    } else if let Some(rest) = uri.strip_prefix("ipfs://") {
        if rest.is_empty() {
            return Err(bad());
        }
        // Reject path-traversal segments — a contract returning
        // `ipfs://Qm../../etc/passwd` (or percent-encoded variants)
        // could trick a misbehaving gateway into serving an unrelated
        // file. The gateway is the last line of defence, but rejecting
        // up-front keeps the contract-vs-our-validator boundary clean.
        // Decode `%2e` (any case) → `.` first so mixed encodings like
        // `.%2e`, `%2e.`, `%2e/` collapse to literal-dot segments before
        // the per-segment match.
        let normalized = rest.to_ascii_lowercase().replace("%2e", ".");
        if normalized.split('/').any(|seg| seg == ".." || seg == ".") {
            return Err(bad());
        }
        return Ok(());
    } else {
        return Err(NftTokenUriError::UnsafeScheme {
            uri: uri.to_owned(),
        });
    };
    let authority = host.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err(bad()); // userinfo masks the host check
    }
    let host_only = authority.split(':').next().unwrap_or("");
    if host_only.is_empty() {
        return Err(bad());
    }
    if !host_only
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err(bad());
    }
    if host_only.parse::<IpAddr>().is_ok() {
        return Err(bad());
    }
    if !host_only.contains('.') {
        return Err(bad()); // reject `localhost` etc. — must be public DNS
    }
    Ok(())
}

#[cfg(test)]
mod tests;
