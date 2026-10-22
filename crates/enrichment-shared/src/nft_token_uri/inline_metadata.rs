//! `token_uri()` values that carry the metadata themselves (task 0654).
//!
//! Some contracts return `data:application/json,{…}` (or `;base64,…`)
//! instead of a link; the metadata is in the string, so nothing is fetched.
//! Read the same way a browser reads a `data:` URL (WHATWG Fetch), via the
//! `data-url` crate.

use data_url::DataUrl;
use serde_json::{Value, json};

use super::client::MAX_BODY_BYTES;
use super::errors::NftTokenUriError;

/// Metadata JSON from a `data:` `token_uri`. A `data:image/…` value is the
/// direct-image convention, as for a fetched `image/*` response.
pub(super) fn metadata_from_data_uri(uri: &str) -> Result<Value, NftTokenUriError> {
    let malformed = || NftTokenUriError::MalformedUri {
        uri: uri.to_owned(),
    };
    let data_url = DataUrl::process(uri).map_err(|_| malformed())?;
    let mime = data_url.mime_type();
    if mime.type_ == "image" {
        return Ok(json!({ "image": uri }));
    }
    if !(mime.matches("application", "json") || mime.matches("text", "json")) {
        return Err(NftTokenUriError::UnsupportedContentType(format!(
            "{}/{}",
            mime.type_, mime.subtype
        )));
    }
    let (body, _fragment) = data_url.decode_to_vec().map_err(|_| malformed())?;
    if body.len() > MAX_BODY_BYTES {
        return Err(NftTokenUriError::BodyTooLarge {
            limit: MAX_BODY_BYTES,
        });
    }
    serde_json::from_slice(&body).map_err(NftTokenUriError::MalformedJson)
}

#[cfg(test)]
mod tests;
