//! `token_uri` value safety check, run before any RPC-returned URI is fetched.

use std::net::IpAddr;

use super::errors::NftTokenUriError;

/// URI safety check: only `https://` (RFC1035 host, no IP literal /
/// userinfo) and `ipfs://` (a CID that decodes, see [`cid_decodes`]) pass.
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
        // A CID that does not decode names content no gateway can serve, so
        // asking one only burns retries; e.g. a contract that glues the token
        // id onto the CID (`ipfs://<cid>15`). Some contracts write the gateway
        // path form `ipfs://ipfs/<cid>`; the CID is then the second segment.
        let content = match rest.strip_prefix("ipfs/") {
            Some(after) => after,
            None => rest,
        };
        let cid = content.split(['/', '?', '#', '%']).next().unwrap_or("");
        if !cid_decodes(cid) {
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

/// Whether `cid` can name content. Checked: a CIDv0 (`Qm` + 44 base58
/// characters, shape only) and a base32 CIDv1 (decoded). Encodings not checked
/// here (base58 `z`, base36 `k`, …) pass and are left to the gateway, as
/// before.
fn cid_decodes(cid: &str) -> bool {
    if cid.starts_with('Q') {
        const BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        return cid.len() == 46 && cid.starts_with("Qm") && cid.chars().all(|c| BASE58.contains(c));
    }
    // CIDv1 in base32: multibase prefix `b` (lower case) or `B` (upper case).
    let Some(base32) = cid.strip_prefix(['b', 'B']) else {
        return true;
    };
    match data_encoding::BASE32_NOPAD.decode(base32.to_ascii_uppercase().as_bytes()) {
        Ok(bytes) => is_cid_v1(&bytes),
        Err(_) => false,
    }
}

/// `<version 1><codec><hash code><digest length><digest>`, each header field an
/// unsigned varint, and the digest exactly as long as it says.
fn is_cid_v1(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    let version = read_varint(&mut rest);
    let codec = read_varint(&mut rest);
    let hash_code = read_varint(&mut rest);
    let digest_len = read_varint(&mut rest);
    version == Some(1)
        && codec.is_some()
        && hash_code.is_some()
        && digest_len == Some(rest.len() as u64)
}

/// Unsigned LEB128 varint, as multiformats use it (at most 9 bytes).
fn read_varint(rest: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for (i, &byte) in rest.iter().enumerate().take(9) {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            *rest = &rest[i + 1..];
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests;
