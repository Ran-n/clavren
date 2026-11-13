// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.190589
//+ Revised: 	2026/11/12 23:57:45.572001
// -----------------------------------------------------------------------------

use chrono::{DateTime, Utc};

use super::*;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).unwrap()
}

fn new(code: &str, name: &str) -> NewProduct {
    NewProduct {
        code: Some(code.into()),
        name: name.into(),
        description: None,
        message_template: None,
        is_active: None,
        accepts_applications: None,
        key_instructions: None,
        offline_days: None,
        price: None,
        test_licence: true,
    }
}

/// `product` stored with the code it was given.
fn create(product: NewProduct) -> Result<Product, ApiError> {
    let code = product.code.clone().unwrap_or_default();
    product.into_product(Uuid::now_v7(), now(), &code, 30)
}

fn existing() -> Product {
    create(new("avalio-lite", "Avalio Lite")).unwrap()
}

#[test]
fn a_new_product_gets_defaults_and_tidy_values() {
    let product =
        create(NewProduct { description: Some("  ".into()), ..new(" Avalio-Lite ", " Avalio Lite ") }).unwrap();
    assert_eq!((product.code.as_str(), product.name.as_str()), ("avalio-lite", "Avalio Lite"));
    assert_eq!(product.description, None);
    assert_eq!(product.message_template, message_template::DEFAULT);
    assert!(product.is_active);
    assert!(!product.accepts_applications, "applications are closed until the administrator opens them");
    assert_eq!((product.created_at, product.updated_at), (now(), now()));
}

#[test]
fn invalid_codes_are_refused() {
    for code in ["-lite", "avalio lite", "avalio_lite", "ñ", &"a".repeat(64)] {
        assert!(create(new(code, "Name")).is_err(), "{code:?}");
    }
    assert!(create(new(&"a".repeat(63), "Name")).is_ok());
}

#[test]
fn a_blank_or_missing_code_is_left_to_be_made() {
    for code in [None, Some(""), Some("   ")] {
        let product = NewProduct { code: code.map(String::from), ..new("x", "Name") };
        assert_eq!(product.typed_code(), None, "{code:?}");
    }
    assert_eq!(new(" Avalio ", "Name").typed_code(), Some("Avalio"));
    assert!(new("", " ").check_name().is_err());
    assert!(new("", &"x".repeat(201)).check_name().is_err());
    assert!(new("", "Avalio").check_name().is_ok());
}

#[test]
fn templates_with_unknown_placeholders_are_refused() {
    let product = NewProduct { message_template: Some("Hi {name}".into()), ..new("a", "A") };
    let error = create(product).unwrap_err();
    assert_eq!(error.code(), "invalid_request");
}

#[test]
fn changes_report_what_changed() {
    let changes = ProductChanges {
        name: Some("Avalio".into()),
        description: Patch::Set("For schools".into()),
        is_active: Some(false),
        ..ProductChanges::default()
    };
    let later = now() + chrono::Duration::hours(1);
    let (product, fields) = changes.apply(existing(), later).unwrap();
    assert_eq!(fields, ["name", "description", "is_active"]);
    assert_eq!(product.description.as_deref(), Some("For schools"));
    assert_eq!(product.updated_at, later);
}

#[test]
fn changes_that_change_nothing_keep_the_product_as_it_was() {
    let before = existing();
    let changes = ProductChanges {
        name: Some(" Avalio Lite ".into()),
        code: Some("AVALIO-LITE".into()),
        ..ProductChanges::default()
    };
    let (product, fields) = changes.apply(before.clone(), now() + chrono::Duration::hours(1)).unwrap();
    assert!(fields.is_empty());
    assert_eq!(product.updated_at, before.updated_at);
}

#[test]
fn a_description_can_be_cleared_with_null_or_blank() {
    let with_description = ProductChanges { description: Patch::Set("x".into()), ..ProductChanges::default() }
        .apply(existing(), now())
        .unwrap()
        .0;
    for clear in [Patch::Clear, Patch::Set("  ".into())] {
        let changes = ProductChanges { description: clear, ..ProductChanges::default() };
        assert_eq!(changes.apply(with_description.clone(), now()).unwrap().0.description, None);
    }
}

#[test]
fn invalid_changes_are_refused() {
    for changes in [
        ProductChanges { name: Some(" ".into()), ..ProductChanges::default() },
        ProductChanges { code: Some("no spaces".into()), ..ProductChanges::default() },
        ProductChanges { message_template: Some("{oops}".into()), ..ProductChanges::default() },
        ProductChanges { message_template: Some("x".repeat(10_001)), ..ProductChanges::default() },
        ProductChanges { description: Patch::Set("x".repeat(2001)), ..ProductChanges::default() },
    ] {
        assert!(changes.apply(existing(), now()).is_err());
    }
}

#[test]
fn a_new_product_comes_with_a_test_licence_unless_asked_not_to() {
    let read = |json: &str| serde_json::from_str::<NewProduct>(json).unwrap().test_licence;
    assert!(read(r#"{"name": "Avalio"}"#));
    assert!(!read(r#"{"name": "Avalio", "test_licence": false}"#));
}

#[test]
fn applications_open_and_close_as_a_change() {
    let open = ProductChanges { accepts_applications: Some(true), ..ProductChanges::default() };
    let (product, fields) = open.apply(existing(), now()).unwrap();
    assert!(product.accepts_applications);
    assert_eq!(fields, ["accepts_applications"]);
    let again = ProductChanges { accepts_applications: Some(true), ..ProductChanges::default() };
    assert!(again.apply(product, now()).unwrap().1.is_empty(), "already open");
}

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
