use super::*;

/// The prices JOIN key contract (views.sql, pinned 2026-06-16):
/// native = ('native','XLM',''), classic = ('credit', code, issuer).
/// A wrong mapping here silently prices legs off the wrong row — the
/// exact failure mode the raw-`prices.assets` join produced (task 0199
/// activation note, bogus 96.4% coverage).
#[test]
fn price_leg_mapping() {
    let native = price_leg(0, None, None);
    assert_eq!(
        (native.kind, native.code.as_str(), native.issuer.as_str()),
        ("native", "XLM", "")
    );
    // Native ignores whatever code/issuer the row carries ('' / surrogate-0 artifacts).
    let native2 = price_leg(0, Some(""), Some(""));
    assert_eq!(native2.kind, "native");

    let usdc = price_leg(
        1,
        Some("USDC"),
        Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"),
    );
    assert_eq!(usdc.kind, "credit");
    assert_eq!(usdc.code, "USDC");
    assert!(usdc.issuer.starts_with('G'));

    let alphanum12 = price_leg(2, Some("WGUARDIAN"), Some("GABC"));
    assert_eq!(alphanum12.kind, "credit");

    // Unpriceable degradations: missing identity parts or unexpected type
    // must match NO prices row (empty kind), never guess.
    assert_eq!(price_leg(1, None, Some("GABC")).kind, "");
    assert_eq!(price_leg(1, Some("USDC"), None).kind, "");
    assert_eq!(price_leg(1, Some(""), Some("GABC")).kind, "");
    assert_eq!(price_leg(3, Some("X"), Some("G")).kind, "");
    assert_eq!(price_leg(9, None, None).kind, "");
}

#[test]
fn usd_helpers() {
    assert_eq!(parse_f64("123.4567890"), Some(123.456789));
    assert_eq!(parse_f64("0"), Some(0.0));
    assert_eq!(parse_f64(""), None);
    assert_eq!(parse_f64("abc"), None);
    assert_eq!(parse_f64("inf"), None, "non-finite rejected");
    assert_eq!(usd_str(1234.5678), "1234.57");
    assert_eq!(usd_str(0.0), "0.00");
    // Sub-cent values must not collapse to "0.00" — a client cannot
    // tell that apart from a genuine zero (fee_revenue lives here).
    assert_eq!(usd_str(0.003), "0.0030");
    assert_eq!(usd_str(0.00009), "0.000090");
    assert_eq!(usd_str(-0.003), "-0.0030");
    // At or above a cent the plain money form still applies.
    assert_eq!(usd_str(0.01), "0.01");
    assert_eq!(usd_str(0.5), "0.50");
    // Fixed 2 decimals on every path — CH's toString(round(x, 2)) would
    // emit "25" / "1.5" / "0" here and split the wire shape between the
    // chart and the detail endpoint.
    assert_eq!(usd_str(25.0), "25.00");
    assert_eq!(usd_str(1.5), "1.50");
}

/// `fee_bps` is basis points: 30 bps = 0.30%, so the divisor is 10 000.
/// A /100 or /1000 slip inflates reported LP earnings 100× / 10×.
#[test]
fn fee_revenue_math() {
    assert_eq!(fee_revenue_usd(1_000_000.0, 30), 3_000.0);
    assert_eq!(fee_revenue_usd(1_000.0, 100), 10.0);
    assert_eq!(fee_revenue_usd(0.0, 30), 0.0);
    assert_eq!(fee_revenue_usd(500.0, 0), 0.0);
}

/// TVL is every leg's reserve × price, and nothing when any leg lacks either —
/// a partial sum would understate the pool while looking like a real number.
#[test]
fn tvl_needs_every_leg_priced_and_reserved() {
    let xlm = price_leg(0, None, None);
    let usdc = price_leg(
        1,
        Some("USDC"),
        Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"),
    );
    let closes: HashMap<PriceLeg, f64> = [(xlm.clone(), 0.5), (usdc.clone(), 1.0)].into();
    let legs = [xlm, usdc];

    assert_eq!(tvl_usd(&[Some("10"), Some("3")], &legs, &closes), Some(8.0));
    assert_eq!(tvl_usd(&[Some("10"), None], &legs, &closes), None);
    // A third leg with no price makes the whole pool unpriced.
    let dai = price_leg(
        1,
        Some("DAI"),
        Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"),
    );
    let three = [legs[0].clone(), legs[1].clone(), dai];
    assert_eq!(
        tvl_usd(&[Some("10"), Some("3"), Some("1")], &three, &closes),
        None
    );
}
