// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/12 23:08:56.973740
//+ Revised: 	2026/11/12 23:57:45.197070
// -----------------------------------------------------------------------------

//! Money: a price is a whole amount in the currency's minor unit (cents for EUR) and an ISO 4217
//! code, never a float, so amounts never drift. The same type serves products (their price per
//! key), licences (what that key was charged), the Overview's sums and, later, payments. The
//! server checks only the code's shape (three uppercase letters); the dashboard only offers real
//! currencies.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::FromRow;

use crate::http::error::ApiError;

/// The largest amount, in minor units: just under one million euros.
pub const MAX_AMOUNT: i64 = 99_999_999;

/// An ISO 4217 code such as `EUR`, checked for shape only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Currency(String);

impl Currency {
    pub fn parse(code: &str) -> Result<Self, ApiError> {
        if code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            Ok(Self(code.to_string()))
        } else {
            Err(ApiError::invalid_request("price currency must be a three-letter ISO 4217 code such as EUR"))
        }
    }

    pub fn euro() -> Self {
        Self("EUR".into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// How many digits its minor unit has: 2 for most, as ISO 4217 lists.
    pub fn digits(&self) -> u32 {
        match self.0.as_str() {
            "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF" | "UGX" | "VND" | "VUV"
            | "XAF" | "XOF" | "XPF" => 0,
            "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
            _ => 2,
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Self::parse(&code).map_err(|error| serde::de::Error::custom(error.message()))
    }
}

/// `{"amount": 1999, "currency": "EUR"}` is €19.99.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, try_from = "Unchecked")]
pub struct Price {
    pub amount: i64,
    pub currency: Currency,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Unchecked {
    amount: i64,
    currency: Currency,
}

impl TryFrom<Unchecked> for Price {
    type Error = String;

    fn try_from(price: Unchecked) -> Result<Self, Self::Error> {
        Self::new(price.amount, price.currency).map_err(|error| error.message().to_string())
    }
}

impl Price {
    pub fn new(amount: i64, currency: Currency) -> Result<Self, ApiError> {
        if (0..=MAX_AMOUNT).contains(&amount) {
            Ok(Self { amount, currency })
        } else {
            Err(ApiError::invalid_request(format!("price amount must be between 0 and {MAX_AMOUNT}")))
        }
    }

    /// The amount as a decimal with the currency's digits, e.g. `19.99`, `1999` (JPY), `1.999` (KWD).
    pub fn decimal(&self) -> String {
        let digits = self.currency.digits();
        if digits == 0 {
            return self.amount.to_string();
        }
        let unit = 10_i64.pow(digits);
        format!("{}.{:0width$}", self.amount / unit, self.amount % unit, width = digits as usize)
    }
}

/// A price as its two database columns, both set or both `NULL` (a check constraint holds them
/// together). Read with `#[sqlx(flatten)]`; serializes as the price, or `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq, FromRow)]
pub struct PriceColumns {
    pub price_amount: Option<i64>,
    pub price_currency: Option<String>,
}

impl PriceColumns {
    pub fn price(&self) -> Option<Price> {
        let (Some(amount), Some(currency)) = (self.price_amount, self.price_currency.as_deref()) else {
            return None;
        };
        // the database's constraints hold the same rules, so a stored price always reads back
        Some(Price { amount, currency: Currency(currency.to_string()) })
    }
}

impl From<Option<Price>> for PriceColumns {
    fn from(price: Option<Price>) -> Self {
        match price {
            Some(price) => Self { price_amount: Some(price.amount), price_currency: Some(price.currency.0) },
            None => Self::default(),
        }
    }
}

impl Serialize for PriceColumns {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.price().serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
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
}
