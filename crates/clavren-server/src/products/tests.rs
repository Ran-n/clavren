// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.190589
//+ Revised: 	2026/11/07 19:37:00.236858
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
    }
}

/// `product` stored with the code it was given.
fn create(product: NewProduct) -> Result<Product, ApiError> {
    let code = product.code.clone().unwrap_or_default();
    product.into_product(Uuid::now_v7(), now(), &code)
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
