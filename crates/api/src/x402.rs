//! Paid twins of three data routes: a caller with no API key and no browser
//! pays USDC for one request through x402 and gets the answer (task 0682; the
//! design is stellar-x402's task 0041).
//!
//! | Route                                              | Twin of                                     |
//! | -------------------------------------------------- | ------------------------------------------- |
//! | `GET /x402/v1/transactions/{hash}`                 | `GET /v1/transactions/{hash}`               |
//! | `GET /x402/v1/contracts/{contract_id}/events`      | `GET /v1/contracts/{contract_id}/events`    |
//! | `GET /x402/v1/contracts/{contract_id}/interface`   | `GET /v1/contracts/{contract_id}/interface` |
//!
//! Each is the keyed route's own handler behind the `stellar-x402` crate's
//! payment layer. Without a payment the layer answers `402` with the terms and
//! the handler does not run. With one, the facilitator verifies it, the handler
//! runs, and only an answer below `400` is settled. The keyed routes answer
//! `200` with nothing in it in three cases, and nobody pays for those:
//! [`sold`] turns an answer marked [`Nothing`] into a `4xx` or `5xx` first.
//!
//! [`mount`] adds the routes after the auth gate's layer and before the
//! edge-secret lock's: no key and no session is asked for, and a buyer still
//! has to come through Cloudflare. They exist only when `X402_PAY_TO` is set
//! and `X402_NETWORK` is the network this deployment indexes
//! ([`Settings::from_env`]): data of one network is never sold for the other's
//! USDC.
//!
//! The facilitator is asked on the first paid-route request, never at cold
//! start (`PaymentGate::lazy`): a facilitator outage costs the paid routes a
//! `5xx` and the rest of the API nothing.

use std::convert::Infallible;
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{MethodRouter, get};
use serde_json::json;
use stellar_x402::axum::{PaidRoute, PaymentEvent, PaymentGate};
use stellar_x402::{Facilitator, Listing, Network, Price};

use crate::common::{cache_control, errors};
use crate::config::AppConfig;
use crate::contracts::handlers::{get_interface, list_events};
use crate::state::AppState;
use crate::transactions::handlers::get_transaction;

/// The longest one call to the facilitator may take. A settlement takes about
/// 6 s; API Gateway gives the whole request 29 s.
const FACILITATOR_TIMEOUT: Duration = Duration::from_secs(25);

type Error = Box<dyn std::error::Error + Send + Sync>;

/// The paid routes' settings.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The Stellar account that is paid. The service holds no key for it.
    pub pay_to: String,
    /// The network the USDC is paid on: the one this deployment indexes.
    pub network: Network,
    pub facilitator_url: String,
    /// USD for one request, as a decimal.
    pub price_usd: String,
}

impl Settings {
    /// The settings from the environment. `None` without `X402_PAY_TO`, and,
    /// with an error logged, when the settings name another network than
    /// `STELLAR_NETWORK_PASSPHRASE`'s: the paid routes then do not exist.
    pub fn from_env() -> Option<Self> {
        let var = |name: &str| std::env::var(name).unwrap_or_default();
        match Self::read(
            &var("X402_PAY_TO"),
            &var("X402_NETWORK"),
            &var("STELLAR_NETWORK_PASSPHRASE"),
            &var("X402_FACILITATOR_URL"),
            &var("X402_PRICE_USD"),
        ) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::error!(%error, "x402: a setting is wrong, the paid routes are not served");
                None
            }
        }
    }

    /// [`Self::from_env`] over its five values; an empty one is unset.
    fn read(
        pay_to: &str,
        network: &str,
        passphrase: &str,
        facilitator_url: &str,
        price_usd: &str,
    ) -> Result<Option<Self>, String> {
        let or = |value: &str, default: &str| match value.trim() {
            "" => default.to_owned(),
            value => value.to_owned(),
        };
        if pay_to.trim().is_empty() {
            return Ok(None);
        }
        let network = match or(network, "stellar:testnet").as_str() {
            "stellar:testnet" => Network::Testnet,
            "stellar:pubnet" => Network::Pubnet,
            other => return Err(format!("X402_NETWORK is {other:?}")),
        };
        let indexed = match passphrase.trim() {
            "Test SDF Network ; September 2015" => Network::Testnet,
            "Public Global Stellar Network ; September 2015" => Network::Pubnet,
            other => return Err(format!("no x402 network for the passphrase {other:?}")),
        };
        if network != indexed {
            return Err(format!(
                "X402_NETWORK is {network} and this deployment indexes {indexed}"
            ));
        }
        Ok(Some(Self {
            pay_to: pay_to.trim().to_owned(),
            network,
            facilitator_url: or(facilitator_url, "https://x402-facilitator.sorobanscan.com"),
            price_usd: or(price_usd, "0.001"),
        }))
    }
}

/// `router` with the paid routes, when there are settings and they are valid.
///
/// A wrong address, price or facilitator URL is logged and leaves `router` as
/// it was: the rest of the API must start whatever is wrong here. The routes
/// are added one by one with `Router::route`, not merged as a router, which
/// would replace the fallback the layers before this call already wrap.
pub fn mount(router: Router, config: &AppConfig, state: AppState) -> Router {
    let Some(settings) = &config.x402 else {
        return router;
    };
    match routes(settings, &config.base_url, state) {
        Ok(paid) => paid
            .into_iter()
            .fold(router, |router, (path, route)| router.route(path, route)),
        Err(error) => {
            tracing::error!(%error, "x402: a setting is wrong, the paid routes are not served");
            router
        }
    }
}

/// The three paid routes, each behind its price.
fn routes(
    settings: &Settings,
    base_url: &str,
    state: AppState,
) -> Result<[(&'static str, MethodRouter); 3], Error> {
    let facilitator =
        Facilitator::new(&settings.facilitator_url)?.with_timeout(FACILITATOR_TIMEOUT);
    // The 402 and the catalog's listing name each route by the public address.
    let gate = PaymentGate::lazy(facilitator)
        .base_url(base_url)
        .on_event(log);
    let paid = |route: MethodRouter<AppState>,
                description: &str,
                listing: Listing|
     -> Result<MethodRouter, Error> {
        let price = gate
            .charge(
                settings.network,
                &settings.pay_to,
                Price::usdc(&settings.price_usd)?,
            )?
            .description(description)
            .mime_type("application/json")
            .listing(listing);
        Ok(sell(route, price, state.clone()))
    };

    let transaction = Listing::query(json!({})).output(json!({
        "hash": "6bd72a6ef6a77a7dc08e501bd4849b6c28ad88cceabb51a4fd105673949412a2",
        "ledger_sequence": 1234567,
        "source_account": "GAZQTB6FZ6O7GPATG3OXCV5PPI3Y62RVF7GEKXF3P53VJPLKLBQSZFKG",
        "successful": true,
        "operation_count": 1,
        "has_soroban": true,
        "heavy_fields_status": "ok"
    }));
    let events = Listing::query(json!({ "limit": 20 })).output(json!({
        "data": [{
            "id": "0005302408224710656-0000000001",
            "transaction_hash": "6bd72a6ef6a77a7dc08e501bd4849b6c28ad88cceabb51a4fd105673949412a2",
            "ledger_sequence": 1234567,
            "successful": true,
            "event_type": "contract",
            "topics": [{ "type": "sym", "value": "transfer" }],
            "data": { "type": "i128", "value": "10000" }
        }],
        "page": { "next_cursor": null, "prev_cursor": null, "limit": 20 }
    }));
    let interface = Listing::query(json!({})).output(json!({
        "contract_id": "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
        "wasm_hash": "a41fc53d6753b6c04eb15b021c55052366a4c8e0e21bc72700f461264ec1350e",
        "interface_metadata": {
            "functions": [{
                "name": "balance",
                "doc": "",
                "inputs": [{ "name": "id", "type_name": "Address" }],
                "outputs": ["i128"]
            }],
            "wasm_byte_len": 12345
        }
    }));

    Ok([
        (
            "/x402/v1/transactions/{hash}",
            paid(
                get(get_transaction),
                "A Stellar transaction with its decoded operations, events and invocation tree",
                transaction,
            )?,
        ),
        (
            "/x402/v1/contracts/{contract_id}/events",
            paid(
                get(list_events),
                "The events a Soroban contract emitted, decoded, newest first, in pages",
                events,
            )?,
        ),
        (
            "/x402/v1/contracts/{contract_id}/interface",
            paid(
                get(get_interface),
                "The public functions of a Soroban contract, with their parameter and return types",
                interface,
            )?,
        ),
    ])
}

/// What happened to a payment, for the logs. The transaction's hash and the
/// facilitator's reason codes only: a payer's payment is never logged.
fn log(event: &PaymentEvent<'_>) {
    match event {
        PaymentEvent::Settled(settled) => {
            let catalog = settled
                .extension_responses
                .as_ref()
                .and_then(|answers| answers.get("bazaar"));
            tracing::info!(transaction = %settled.transaction, ?catalog, "x402: payment settled");
        }
        PaymentEvent::SettleRefused(settled) => tracing::warn!(
            reason = ?settled.error_reason,
            transaction = %settled.transaction,
            "x402: the route answered and the payment was not settled"
        ),
        PaymentEvent::VerifyRefused(verified) => {
            tracing::info!(reason = ?verified.invalid_reason, "x402: payment refused");
        }
        PaymentEvent::FacilitatorFailed(error) => {
            tracing::error!(%error, "x402: the facilitator failed");
        }
        PaymentEvent::Unsupported(network) => tracing::error!(
            %network,
            "x402: the facilitator does not settle on X402_NETWORK, every paid request is 500"
        ),
        _ => {}
    }
}

/// Marks a keyed route's `200` as having nothing in it; the values are the
/// status and the code the paid twin answers in its place. The keyed route
/// itself ignores it: a response extension is never sent.
#[derive(Clone, Copy)]
pub(crate) struct Nothing(pub(crate) StatusCode, pub(crate) &'static str);

/// `route` behind `price`. The payment layer goes on last, so it is the
/// outermost: an unpaid request is answered before the handler runs, and
/// [`sold`] has turned an answer marked [`Nothing`] into an error before the
/// layer decides whether to settle.
fn sell<S>(route: MethodRouter<S>, price: PaidRoute, state: S) -> MethodRouter
where
    S: Clone + Send + Sync + 'static,
{
    route
        .layer::<_, Infallible>(axum::middleware::map_response(sold))
        .route_layer(price)
        .with_state(state)
}

/// The keyed route's answer, as the paid twin sends it: an error in place of
/// an answer marked [`Nothing`], so the payment layer does not settle, and
/// never stored by a cache, since the next caller has not paid.
async fn sold(mut response: Response) -> Response {
    if let Some(Nothing(status, code)) = response.extensions().get::<Nothing>().copied() {
        return errors::refusal(
            status,
            code,
            "there is no data to sell for this request, so nothing was charged",
        );
    }
    cache_control::attach(&mut response, cache_control::NO_STORE);
    response
}

#[cfg(test)]
mod tests;
