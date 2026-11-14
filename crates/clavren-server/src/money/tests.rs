// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:30.932191
//+ Revised: 	2026/11/14 11:36:30.932191
// -----------------------------------------------------------------------------

use super::*;

fn eur(amount: i64) -> Result<Price, ApiError> {
    Price::new(amount, Currency::euro())
}

#[test]
fn amounts_go_from_0_to_99999999() {
    for amount in [0, 1999, MAX_AMOUNT] {
        assert!(eur(amount).is_ok(), "{amount}");
    }
    for amount in [-1, MAX_AMOUNT + 1] {
        assert_eq!(eur(amount).unwrap_err().code(), "invalid_request", "{amount}");
    }
}

#[test]
fn currencies_are_three_uppercase_letters() {
    assert_eq!(Currency::parse("EUR").unwrap().as_str(), "EUR");
    for code in ["eur", "EU", "EURO", "€UR", "E1R", ""] {
        assert!(Currency::parse(code).is_err(), "{code:?}");
    }
}

#[test]
fn decimals_follow_the_currencys_digits() {
    assert_eq!(eur(1999).unwrap().decimal(), "19.99");
    assert_eq!(eur(5).unwrap().decimal(), "0.05");
    assert_eq!(eur(0).unwrap().decimal(), "0.00");
    assert_eq!(Price::new(1999, Currency::parse("JPY").unwrap()).unwrap().decimal(), "1999");
    assert_eq!(Price::new(1999, Currency::parse("KWD").unwrap()).unwrap().decimal(), "1.999");
}

#[test]
fn prices_read_and_write_json_with_their_checks() {
    let price: Price = serde_json::from_str(r#"{"amount": 1999, "currency": "EUR"}"#).unwrap();
    assert_eq!(price, eur(1999).unwrap());
    assert_eq!(serde_json::to_value(&price).unwrap(), serde_json::json!({ "amount": 1999, "currency": "EUR" }));
    for bad in [
        r#"{"amount": -1, "currency": "EUR"}"#,
        r#"{"amount": 1, "currency": "eur"}"#,
        r#"{"amount": 1.5, "currency": "EUR"}"#,
        r#"{"amount": 1}"#,
        r#"{"amount": 1, "currency": "EUR", "tax": 0}"#,
    ] {
        assert!(serde_json::from_str::<Price>(bad).is_err(), "{bad}");
    }
}

#[test]
fn columns_hold_a_price_or_nothing() {
    let columns = PriceColumns::from(Some(eur(1999).unwrap()));
    assert_eq!((columns.price_amount, columns.price_currency.as_deref()), (Some(1999), Some("EUR")));
    assert_eq!(columns.price(), Some(eur(1999).unwrap()));
    assert_eq!(PriceColumns::from(None).price(), None);
    assert_eq!(serde_json::to_value(PriceColumns::default()).unwrap(), serde_json::Value::Null);
}
