use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Internal cursor payload — never exposed to clients.
#[derive(Debug, Serialize, Deserialize)]
struct CursorPayload {
    after_id: i64,
}

pub fn encode(last_id: i64) -> String {
    let payload = CursorPayload { after_id: last_id };
    let json = serde_json::to_vec(&payload).expect("cursor serialization");
    URL_SAFE_NO_PAD.encode(&json)
}

pub fn decode(cursor: &str) -> Result<i64, AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| AppError::BadRequest("invalid cursor".into()))?;
    let payload: CursorPayload =
        serde_json::from_slice(&bytes).map_err(|_| AppError::BadRequest("invalid cursor".into()))?;
    Ok(payload.after_id)
}
