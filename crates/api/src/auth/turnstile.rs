//! Cloudflare **Turnstile** token verification (siteverify) for the free tier
//! (task 0277 paid-API). The SPA solves a Turnstile challenge, sends the token
//! to `/auth/session`, and the backend confirms it with Cloudflare here before
//! minting a session JWT.

use serde::Deserialize;

const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

#[derive(Deserialize)]
struct SiteverifyResponse {
    success: bool,
}

/// POST `secret` + `token` to Cloudflare siteverify. `true` iff Cloudflare
/// confirms the token. Any network/parse error → `false` (fail-closed).
pub async fn verify(client: &reqwest::Client, secret: &str, token: &str) -> bool {
    let resp = client
        .post(SITEVERIFY_URL)
        .form(&[("secret", secret), ("response", token)])
        .send()
        .await;

    match resp {
        Ok(r) => r
            .json::<SiteverifyResponse>()
            .await
            .map(|b| b.success)
            .unwrap_or(false),
        Err(_) => false,
    }
}
