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

#[tokio::test]
async fn an_answer_marked_as_nothing_is_the_error_it_names() {
    let mut empty = Response::new(axum::body::Body::empty());
    empty
        .extensions_mut()
        .insert(Nothing(StatusCode::NOT_FOUND, errors::NO_DATA));

    let sent = sold(empty).await;

    assert_eq!(sent.status(), StatusCode::NOT_FOUND);
    assert_eq!(sent.headers()["cache-control"], "no-store");
    let body = axum::body::to_bytes(sent.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "no_data");
}

#[tokio::test]
async fn an_answer_with_data_is_sent_and_never_cached() {
    let mut full = Response::new(axum::body::Body::empty());
    cache_control::attach(&mut full, cache_control::LONG);

    let sent = sold(full).await;

    assert_eq!(sent.status(), StatusCode::OK);
    assert_eq!(sent.headers()["cache-control"], "no-store");
}
