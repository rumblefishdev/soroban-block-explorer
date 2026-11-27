use super::*;

const PAY_TO: &str = "GAE63ENKLHRYHJP4V3PYYSPIAKW7DLZ3JOYGS6TQSSJGDH5MGQFVZFVC";
const TESTNET: &str = "Test SDF Network ; September 2015";
const PUBNET: &str = "Public Global Stellar Network ; September 2015";

#[test]
fn without_an_account_to_pay_there_are_no_settings() {
    assert!(Settings::read("", "", TESTNET, "", "").unwrap().is_none());
    assert!(
        Settings::read("  ", "stellar:testnet", TESTNET, "", "")
            .unwrap()
            .is_none()
    );
}

#[test]
fn unset_values_take_the_testnet_defaults() {
    let settings = Settings::read(PAY_TO, "", TESTNET, "", "")
        .unwrap()
        .unwrap();

    assert_eq!(settings.pay_to, PAY_TO);
    assert_eq!(settings.network, Network::Testnet);
    assert_eq!(
        settings.facilitator_url,
        "https://x402-facilitator.sorobanscan.com"
    );
    assert_eq!(settings.price_usd, "0.001");
}

#[test]
fn data_of_one_network_is_never_sold_for_the_others_usdc() {
    // Free testnet USDC must not buy mainnet data, and the reverse makes no sense.
    for (network, passphrase) in [
        ("stellar:testnet", PUBNET),
        ("", PUBNET),
        ("stellar:pubnet", TESTNET),
        ("stellar:testnet", "Standalone Network ; February 2017"),
        ("stellar:testnet", ""),
        ("stellar:mainnet", PUBNET),
    ] {
        let read = Settings::read(PAY_TO, network, passphrase, "", "");
        assert!(read.is_err(), "{network:?} with {passphrase:?}");
    }
    let pubnet = Settings::read(PAY_TO, "stellar:pubnet", PUBNET, "", "")
        .unwrap()
        .unwrap();
    assert_eq!(pubnet.network, Network::Pubnet);
}

/// A facilitator on loopback that finds every payment valid and settles it.
async fn facilitator() -> wiremock::MockServer {
    use wiremock::matchers::path;
    use wiremock::{Mock, ResponseTemplate};
    let server = wiremock::MockServer::start().await;
    let answers = [
        (
            "/supported",
            json!({ "kinds": [{ "x402Version": 2, "scheme": "exact", "network": "stellar:testnet" }] }),
        ),
        ("/verify", json!({ "isValid": true, "payer": PAY_TO })),
        (
            "/settle",
            json!({
                "success": true,
                "payer": PAY_TO,
                "transaction": "6bd72a6ef6a77a7dc08e501bd4849b6c28ad88cceabb51a4fd105673949412a2",
                "network": "stellar:testnet"
            }),
        ),
    ];
    for (at, answer) in answers {
        Mock::given(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer))
            .mount(&server)
            .await;
    }
    server
}

/// How many times the facilitator was asked to settle.
async fn settled(facilitator: &wiremock::MockServer) -> usize {
    let asked = facilitator.received_requests().await.unwrap();
    asked.iter().filter(|r| r.url.path() == "/settle").count()
}

/// Asks `uri` for its terms, then sends the request again with a payment.
async fn pay(app: &Router, uri: &str) -> Response {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use tower::ServiceExt;
    let ask = |payment: Option<String>| {
        let mut request = axum::http::Request::builder().uri(uri);
        if let Some(payment) = payment {
            request = request.header("PAYMENT-SIGNATURE", payment);
        }
        request.body(axum::body::Body::empty()).unwrap()
    };
    let unpaid = app.clone().oneshot(ask(None)).await.unwrap();
    assert_eq!(unpaid.status(), StatusCode::PAYMENT_REQUIRED);
    let terms: serde_json::Value = serde_json::from_slice(
        &STANDARD
            .decode(unpaid.headers()["payment-required"].as_bytes())
            .unwrap(),
    )
    .unwrap();
    let payment = json!({ "x402Version": 2, "accepted": terms["accepts"][0], "payload": {} });
    let payment = STANDARD.encode(payment.to_string());
    app.clone().oneshot(ask(Some(payment))).await.unwrap()
}

#[tokio::test]
async fn an_answer_marked_as_nothing_is_not_settled_and_one_with_data_is() {
    use axum::response::IntoResponse;
    async fn nothing() -> Response {
        let mut empty = axum::Json(json!({ "data": [] })).into_response();
        empty
            .extensions_mut()
            .insert(Nothing(StatusCode::NOT_FOUND, errors::NO_DATA));
        empty
    }
    async fn data() -> Response {
        let mut full = axum::Json(json!({ "data": [1] })).into_response();
        cache_control::attach(&mut full, cache_control::LONG);
        full
    }
    let facilitator = facilitator().await;
    let gate = PaymentGate::lazy(Facilitator::new(&facilitator.uri()).unwrap());
    let price = || {
        gate.charge(Network::Testnet, PAY_TO, Price::usdc("0.001").unwrap())
            .unwrap()
    };
    let app = Router::new()
        .route("/nothing", sell(get(nothing), price(), ()))
        .route("/data", sell(get(data), price(), ()));

    let sent = pay(&app, "/nothing").await;
    assert_eq!(sent.status(), StatusCode::NOT_FOUND);
    assert_eq!(sent.headers()["cache-control"], "no-store");
    let body = axum::body::to_bytes(sent.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "no_data");
    assert_eq!(settled(&facilitator).await, 0);

    let sent = pay(&app, "/data").await;
    assert_eq!(sent.status(), StatusCode::OK);
    assert!(sent.headers().contains_key("payment-response"));
    let cache = sent.headers()["cache-control"].to_str().unwrap();
    assert!(cache.starts_with("no-store"), "{cache}");
    assert_eq!(settled(&facilitator).await, 1);
}
