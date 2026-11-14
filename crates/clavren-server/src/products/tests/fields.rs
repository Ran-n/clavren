// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:59:41.959270
//+ Revised: 	2026/11/14 11:59:41.959270
// -----------------------------------------------------------------------------

//! Key instructions, offline days and the price: fields with rules of their own.

use super::*;

#[test]
fn key_instructions_are_optional_tidy_and_limited() {
    let without = existing();
    assert_eq!(without.key_instructions, None);
    let with = create(NewProduct {
        key_instructions: Some(
            "  Write to sales@example.com.

Schools get one free.  "
                .into(),
        ),
        ..new("a", "A")
    })
    .unwrap();
    assert_eq!(
        with.key_instructions.as_deref(),
        Some(
            "Write to sales@example.com.

Schools get one free."
        )
    );
    let blank = create(NewProduct {
        key_instructions: Some(
            " 
 "
            .into(),
        ),
        ..new("b", "B")
    })
    .unwrap();
    assert_eq!(blank.key_instructions, None, "blank text is stored as none");
    let long = create(NewProduct { key_instructions: Some("x".repeat(2001)), ..new("c", "C") });
    assert_eq!(long.unwrap_err().code(), "invalid_request");
    assert!(create(NewProduct { key_instructions: Some("x".repeat(2000)), ..new("d", "D") }).is_ok());
}

#[test]
fn changing_key_instructions_reports_the_field_once() {
    let set = ProductChanges { key_instructions: Patch::Set("Buy at example.com".into()), ..ProductChanges::default() };
    let (product, fields) = set.apply(existing(), now()).unwrap();
    assert_eq!(fields, ["key_instructions"]);
    let same =
        ProductChanges { key_instructions: Patch::Set("Buy at example.com".into()), ..ProductChanges::default() };
    let (_, fields) = same.apply(product.clone(), now()).unwrap();
    assert!(fields.is_empty(), "the same text changes nothing");
    let blank = ProductChanges { key_instructions: Patch::Set("  ".into()), ..ProductChanges::default() };
    let (cleared, fields) = blank.apply(product.clone(), now()).unwrap();
    assert_eq!((cleared.key_instructions, fields), (None, vec!["key_instructions"]));
    let long = ProductChanges { key_instructions: Patch::Set("x".repeat(2001)), ..ProductChanges::default() };
    assert!(long.apply(product, now()).is_err());
}

#[test]
fn offline_days_default_to_the_servers_and_stay_within_2_to_3650() {
    assert_eq!(existing().offline_days, 30, "the server's default");
    let week = create(NewProduct { offline_days: Some(7), ..new("a", "A") }).unwrap();
    assert_eq!(week.offline_days, 7);
    for days in [1, 0, -5, 3651] {
        let refused = create(NewProduct { offline_days: Some(days), ..new("b", "B") });
        assert_eq!(refused.unwrap_err().code(), "invalid_request", "{days}");
    }
    for days in [2, 3650] {
        assert!(create(NewProduct { offline_days: Some(days), ..new("c", "C") }).is_ok(), "{days}");
    }
}

#[test]
fn changing_offline_days_reports_the_field_and_checks_the_range() {
    let changes = ProductChanges { offline_days: Some(7), ..ProductChanges::default() };
    let (product, fields) = changes.apply(existing(), now()).unwrap();
    assert_eq!((product.offline_days, fields), (7, vec!["offline_days"]));
    let same = ProductChanges { offline_days: Some(7), ..ProductChanges::default() };
    assert!(same.apply(product.clone(), now()).unwrap().1.is_empty());
    let untouched = ProductChanges::default().apply(product.clone(), now()).unwrap().0;
    assert_eq!(untouched.offline_days, 7, "left out, it stays");
    let refused = ProductChanges { offline_days: Some(1), ..ProductChanges::default() };
    assert!(refused.apply(product, now()).is_err());
}

#[test]
fn offline_days_must_be_a_whole_number_when_given() {
    for body in [r#"{"offline_days":null}"#, r#"{"offline_days":2.5}"#, r#"{"offline_days":"x"}"#] {
        assert!(serde_json::from_str::<ProductChanges>(body).is_err(), "{body}");
    }
    let absent: ProductChanges = serde_json::from_str("{}").unwrap();
    assert_eq!(absent.offline_days, None);
}

#[test]
fn a_price_is_set_changed_and_cleared_as_one_field() {
    use crate::money::Currency;
    let price = Price::new(1999, Currency::euro()).unwrap();
    let priced = create(NewProduct { price: Some(price.clone()), ..new("a", "A") }).unwrap();
    assert_eq!(priced.price.price(), Some(price.clone()));
    assert_eq!(existing().price.price(), None, "no price unless given");
    let other = Price::new(2500, Currency::euro()).unwrap();
    let changes = ProductChanges { price: Patch::Set(other.clone()), ..ProductChanges::default() };
    let (changed, fields) = changes.apply(priced.clone(), now()).unwrap();
    assert_eq!((changed.price.price(), fields), (Some(other), vec!["price"]));
    let same = ProductChanges { price: Patch::Set(price), ..ProductChanges::default() };
    assert!(same.apply(priced.clone(), now()).unwrap().1.is_empty());
    let cleared = ProductChanges { price: Patch::Clear, ..ProductChanges::default() };
    let (cleared, fields) = cleared.apply(priced, now()).unwrap();
    assert_eq!((cleared.price.price(), fields), (None, vec!["price"]));
}
