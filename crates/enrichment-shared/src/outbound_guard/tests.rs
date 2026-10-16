use std::error::Error;
use std::net::{IpAddr, SocketAddr};

use super::*;

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn sock(s: &str) -> SocketAddr {
    SocketAddr::new(ip(s), 0)
}

#[test]
fn public_addresses_pass() {
    for addr in [
        "8.8.8.8",
        "1.1.1.1",
        "104.16.0.1",
        "11.0.0.0",
        // Neighbours of the private ranges, one past each edge.
        "9.255.255.255",
        "172.15.255.255",
        "172.32.0.0",
        "192.167.255.255",
        "192.169.0.0",
        "100.63.255.255",
        "100.128.0.0",
        "169.253.255.255",
        "169.255.0.0",
        "126.255.255.255",
        "128.0.0.0",
        "223.255.255.255",
        "2606:4700:4700::1111",
        "2a00:1450:4001::1",
        // An IPv4-mapped public address is that public address.
        "::ffff:8.8.8.8",
    ] {
        assert!(is_public(ip(addr)), "{addr} should be public");
    }
}

#[test]
fn non_public_addresses_are_refused() {
    for addr in [
        // IPv4: loopback, RFC 1918, link-local (instance metadata), CGNAT
        // (Alibaba metadata), this-network, broadcast, benchmarking,
        // documentation, reserved, multicast.
        "127.0.0.1",
        "127.255.255.254",
        "10.0.0.1",
        "10.255.255.255",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.0.1",
        "192.168.255.255",
        "169.254.169.254",
        "169.254.0.0",
        "100.64.0.1",
        "100.100.100.200",
        "100.127.255.255",
        "0.0.0.0",
        "0.1.2.3",
        "255.255.255.255",
        "198.18.0.1",
        "192.0.2.1",
        "203.0.113.7",
        "240.0.0.1",
        "224.0.0.1",
        "239.255.255.250",
        // IPv6: loopback, unspecified, link-local, unique-local (AWS IPv6
        // metadata), documentation, multicast.
        "::1",
        "::",
        "fe80::1",
        "fc00::1",
        "fd00:ec2::254",
        "2001:db8::1",
        "ff02::1",
        // IPv4 carried inside IPv6: mapped, NAT64, 6to4.
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
        "::ffff:10.0.0.1",
        "64:ff9b::7f00:1",
        "2002:7f00:1::",
    ] {
        assert!(!is_public(ip(addr)), "{addr} should be refused");
    }
}

#[test]
fn one_non_public_address_refuses_the_whole_host() {
    let err = check_addrs(
        "mixed.example",
        &[sock("8.8.8.8"), sock("10.0.0.1"), sock("1.1.1.1")],
    )
    .unwrap_err();
    assert_eq!(err.host, "mixed.example");
    assert_eq!(err.addr, ip("10.0.0.1"));
    assert_eq!(
        err.to_string(),
        "refused mixed.example: it resolves to 10.0.0.1, which is not a public address"
    );
}

#[test]
fn all_public_addresses_pass_the_host() {
    assert!(check_addrs("ok.example", &[sock("8.8.8.8"), sock("2606:4700::1")]).is_ok());
}

fn refusal(err: &reqwest::Error) -> Option<&HostRefused> {
    std::iter::successors(Some(err as &dyn Error), |e| (*e).source())
        .find_map(|e| e.downcast_ref::<HostRefused>())
}

#[tokio::test]
async fn a_name_resolving_to_loopback_is_refused_before_any_connection() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(wiremock::ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let url = format!("http://localhost:{}/", server.address().port());

    // Control: without the guard the same URL reaches the server.
    let plain = reqwest::Client::new();
    assert_eq!(plain.get(&url).send().await.unwrap().status(), 200);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    let guarded = reqwest::Client::builder()
        .dns_resolver(resolver())
        .build()
        .unwrap();
    let err = guarded.get(&url).send().await.unwrap_err();
    let refused = refusal(&err).expect("the guard's refusal is in the error chain");
    assert_eq!(refused.host, "localhost");
    assert!(refused.addr.is_loopback());
    // Nothing more reached the server.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_name_that_does_not_resolve_is_an_error() {
    let guarded = reqwest::Client::builder()
        .dns_resolver(resolver())
        .build()
        .unwrap();
    let err = guarded
        .get("http://no-such-host.invalid/")
        .send()
        .await
        .unwrap_err();
    assert!(err.is_connect());
    assert!(refusal(&err).is_none());
}
