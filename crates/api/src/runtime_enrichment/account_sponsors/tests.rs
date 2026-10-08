use super::*;
use stellar_xdr::{
    AccountEntry, AccountEntryExtensionV1, AccountEntryExtensionV2, AccountEntryExtensionV2Ext,
    Liabilities, SequenceNumber, Signer, SignerKey, SponsorshipDescriptor, String32, Thresholds,
    TrustLineEntry, TrustLineEntryExt, Uint256,
};

const WALLET: &str = "GAUA7XL5K54CC2DDGP77FJ2YBHRJLT36CPZDXWPM6MP7MANOGG77PNJU";
const SPONSOR_A: &str = "GCUISJEWU2TZ4QIJNGNVU4BSZ5CQS3KE6A3N3ETOV7XHCBVO4GLTLGOQ";
const SPONSOR_B: &str = "GCNPDPJLTEAPL2FOAYXSSYF6VB6EPP2KFUMXGAJKLIIC3XQLWIY6GQLX";
const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

/// The `extXdr` mainnet RPC returned for `GA3WEM…UV76` on 2026-10-07.
#[test]
fn sponsor_comes_from_the_entry_extension() {
    let ext = "AAAAAQAAAAEAAAAAKA/dfVd4IWhjM//yp1gJ4pXPfhPyO9ns8x/2Aa4xv/cAAAAA";
    assert_eq!(sponsor_of(Some(ext)).unwrap().as_deref(), Some(WALLET));
    assert_eq!(sponsor_of(None).unwrap(), None);
}

/// `GBEEFP…CT56` on mainnet, 2026-10-07: the account and two signers paid by
/// one sponsor, a third signer by another, the USDC trustline by the first —
/// 6 reserves, the account's `num_sponsored`.
#[test]
fn every_sponsored_entry_is_listed_with_its_sponsor() {
    let signer = |b: u8| Signer {
        key: SignerKey::Ed25519(Uint256([b; 32])),
        weight: 1,
    };
    let account = LedgerEntryData::Account(AccountEntry {
        account_id: id("GBEEFPTTFGFCGZYMAFU7XLUBCGPHIM7B2HUXCCAUBKCDPGFITY6ZCT56"),
        balance: 0,
        seq_num: SequenceNumber(1),
        num_sub_entries: 4,
        inflation_dest: None,
        flags: 0,
        home_domain: String32::default(),
        thresholds: Thresholds([1, 0, 0, 0]),
        signers: vec![signer(1), signer(2), signer(3)].try_into().unwrap(),
        ext: AccountEntryExt::V1(AccountEntryExtensionV1 {
            liabilities: Liabilities {
                buying: 0,
                selling: 0,
            },
            ext: AccountEntryExtensionV1Ext::V2(AccountEntryExtensionV2 {
                num_sponsored: 6,
                num_sponsoring: 0,
                signer_sponsoring_i_ds: vec![
                    SponsorshipDescriptor(Some(id(SPONSOR_A))),
                    SponsorshipDescriptor(Some(id(SPONSOR_A))),
                    SponsorshipDescriptor(Some(id(SPONSOR_B))),
                ]
                .try_into()
                .unwrap(),
                ext: AccountEntryExtensionV2Ext::V0,
            }),
        }),
    });
    let usdc = LedgerEntryData::Trustline(TrustLineEntry {
        account_id: id("GBEEFPTTFGFCGZYMAFU7XLUBCGPHIM7B2HUXCCAUBKCDPGFITY6ZCT56"),
        asset: trustline_asset("USDC", USDC_ISSUER).unwrap(),
        balance: 0,
        limit: i64::MAX,
        flags: 1,
        ext: TrustLineEntryExt::V0,
    });

    let out = sponsors(vec![
        (account, Some(SPONSOR_A.into())),
        (usdc, Some(SPONSOR_A.into())),
    ])
    .unwrap();

    assert_eq!(out.num_sponsored, 6);
    let listed: Vec<(&str, u32, &str)> = out
        .entries
        .iter()
        .map(|e| (e.kind, e.reserves, e.sponsor.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            ("account", 2, SPONSOR_A),
            ("signer", 1, SPONSOR_A),
            ("signer", 1, SPONSOR_A),
            ("signer", 1, SPONSOR_B),
            ("trustline", 1, SPONSOR_A),
        ]
    );
    assert_eq!(out.entries.iter().map(|e| e.reserves).sum::<u32>(), 6);
    assert_eq!(
        out.entries[4].asset.as_deref(),
        Some(&*format!("USDC-{USDC_ISSUER}"))
    );
}

/// An entry nobody sponsors is not listed; no account entry means no answer.
#[test]
fn unsponsored_entries_are_skipped_and_a_missing_account_is_none() {
    let usdc = LedgerEntryData::Trustline(TrustLineEntry {
        account_id: id(WALLET),
        asset: trustline_asset("USDC", USDC_ISSUER).unwrap(),
        balance: 0,
        limit: i64::MAX,
        flags: 1,
        ext: TrustLineEntryExt::V0,
    });
    assert_eq!(sponsors(vec![(usdc, None)]), None);
}

/// A stored code that is no asset code names no trustline: refused, so the
/// caller drops it instead of failing the answer.
#[test]
fn a_code_that_is_no_asset_code_is_refused() {
    assert!(trustline_asset("USDC", USDC_ISSUER).is_ok());
    assert!(trustline_asset("yXLM2025ABCD", USDC_ISSUER).is_ok());
    assert!(trustline_asset("yUSDC", USDC_ISSUER).is_ok());
    assert!(trustline_asset("", USDC_ISSUER).is_err());
    assert!(trustline_asset("US-DC", USDC_ISSUER).is_err());
    assert!(trustline_asset("ABCDEFGHIJKLM", USDC_ISSUER).is_err());
}

// ---- the RPC calls, against a local endpoint ------------------------------

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use stellar_xdr::{LedgerEntryExtensionV1, LedgerEntryExtensionV1Ext};

/// `GA3WEM…`'s account entry, sponsored by the wallet, as one RPC entry.
fn account_entry_json() -> serde_json::Value {
    let data = LedgerEntryData::Account(AccountEntry {
        account_id: id("GA3WEM32FFWX2DDBQDQIZM6N27MK2MRJ6WKJCUHEUMSYOGHVE6TZUV76"),
        balance: 0,
        seq_num: SequenceNumber(1),
        num_sub_entries: 1,
        inflation_dest: None,
        flags: 0,
        home_domain: String32::default(),
        thresholds: Thresholds([1, 0, 0, 0]),
        signers: Default::default(),
        ext: AccountEntryExt::V1(AccountEntryExtensionV1 {
            liabilities: Liabilities {
                buying: 0,
                selling: 0,
            },
            ext: AccountEntryExtensionV1Ext::V2(AccountEntryExtensionV2 {
                num_sponsored: 3,
                num_sponsoring: 0,
                signer_sponsoring_i_ds: Default::default(),
                ext: AccountEntryExtensionV2Ext::V0,
            }),
        }),
    });
    let ext = LedgerEntryExt::V1(LedgerEntryExtensionV1 {
        sponsoring_id: SponsorshipDescriptor(Some(id(WALLET))),
        ext: LedgerEntryExtensionV1Ext::V0,
    });
    serde_json::json!({
        "xdr": BASE64.encode(data.to_xdr(Limits::none()).unwrap()),
        "extXdr": BASE64.encode(ext.to_xdr(Limits::none()).unwrap()),
    })
}

/// An RPC that answers its N-th call with `answers[N]` (status, body) and
/// counts the calls.
async fn rpc(answers: Vec<(u16, serde_json::Value)>) -> (String, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let app = axum::Router::new().route(
        "/",
        axum::routing::post(move || {
            let n = seen.fetch_add(1, Ordering::SeqCst);
            let (status, body) = answers[n.min(answers.len() - 1)].clone();
            async move {
                (
                    axum::http::StatusCode::from_u16(status).unwrap(),
                    axum::Json(body),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/"), calls)
}

fn answer(entries: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": { "entries": entries } })
}

/// 250 trustlines: the account and 199 in the first call, the rest in a
/// second.
fn many_trustlines() -> Vec<(String, String)> {
    (0..250)
        .map(|i| (format!("T{i}"), USDC_ISSUER.to_string()))
        .collect()
}

/// No account entry in the first call: nothing to list, and no second call.
#[tokio::test]
async fn no_account_stops_after_the_first_call() {
    let (url, calls) = rpc(vec![(200, answer(vec![]))]).await;
    let fetcher = AccountSponsorsFetcher::with_urls(vec![url]);

    let out = fetcher.fetch(WALLET, &many_trustlines()).await.unwrap();

    assert_eq!(out, None);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// A later call failing keeps what the first one read.
#[tokio::test]
async fn a_failed_later_call_keeps_what_was_read() {
    let (url, calls) = rpc(vec![
        (200, answer(vec![account_entry_json()])),
        (503, answer(vec![])),
    ])
    .await;
    let fetcher = AccountSponsorsFetcher::with_urls(vec![url]);

    let out = fetcher
        .fetch(WALLET, &many_trustlines())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(out.num_sponsored, 3);
    assert_eq!(out.entries.len(), 1);
    assert_eq!(
        (out.entries[0].kind, out.entries[0].sponsor.as_str()),
        ("account", WALLET)
    );
}
