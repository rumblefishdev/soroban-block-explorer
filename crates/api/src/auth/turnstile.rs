//! Cloudflare **Turnstile** token verification (siteverify) for the free tier
//! (task 0277 paid-API). The SPA solves a Turnstile challenge, sends the token
//! to `/auth/session`, and the backend confirms it with Cloudflare here before
//! minting a session JWT.

use serde::Deserialize;

const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

#[derive(Deserialize)]
struct SiteverifyResponse {
    success: bool,
    /// Why Cloudflare refused the token (`timeout-or-duplicate`,
    /// `invalid-input-response`, …); empty on success.
    #[serde(rename = "error-codes", default)]
    error_codes: Vec<String>,
}

/// POST `secret` + `token` to Cloudflare siteverify. `true` iff Cloudflare
/// confirms the token. Any network/parse error → `false` (fail-closed).
/// Every `false` is logged with its cause: a visitor who cannot get a session
/// sees no data, and the log is the only place we learn why.
pub async fn verify(client: &reqwest::Client, secret: &str, token: &str) -> bool {
    let resp = client
        .post(SITEVERIFY_URL)
        .form(&[("secret", secret), ("response", token)])
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("turnstile siteverify request failed: {e}");
            return false;
        }
    };
    let status = resp.status();
    match resp.json::<SiteverifyResponse>().await {
        Ok(body) if body.success => true,
        Ok(body) => {
            tracing::warn!("turnstile verification refused: {:?}", body.error_codes);
            false
        }
        Err(e) => {
            tracing::warn!("turnstile siteverify response unreadable (HTTP {status}): {e}");
            false
        }
    }
}
