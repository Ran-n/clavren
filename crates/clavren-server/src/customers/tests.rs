// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:30.737610
//+ Revised: 	2026/11/14 11:36:30.737610
// -----------------------------------------------------------------------------

use super::*;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).unwrap()
}

fn ana() -> Customer {
    let new =
        NewCustomer { name: " Ana López ".into(), email: Some(" Ana@Example.com ".into()), notes: Some(" ".into()) };
    new.into_customer(Uuid::now_v7(), now()).unwrap()
}

#[test]
fn a_new_customer_is_tidied() {
    let customer = ana();
    assert_eq!(customer.name, "Ana López");
    assert_eq!(customer.email.as_deref(), Some("Ana@Example.com"), "the address is kept as typed");
    assert_eq!(customer.notes, None);
}

#[test]
fn invalid_customers_are_refused() {
    for (name, email) in [("", None), ("Ana", Some("not an address")), (&*"x".repeat(201), None)] {
        let new = NewCustomer { name: name.into(), email: email.map(String::from), notes: None };
        assert!(new.into_customer(Uuid::now_v7(), now()).is_err(), "{name:?} {email:?}");
    }
}

#[test]
fn changes_report_what_changed() {
    let later = now() + chrono::Duration::hours(1);
    let changes =
        CustomerChanges { email: Patch::Clear, notes: Patch::Set("Paid by card".into()), ..Default::default() };
    let (customer, fields) = changes.apply(ana(), later).unwrap();
    assert_eq!(fields, ["email", "notes"]);
    assert_eq!((customer.email, customer.notes.as_deref(), customer.updated_at), (None, Some("Paid by card"), later));
}

#[test]
fn no_real_change_keeps_the_customer_as_it_was() {
    let before = ana();
    let changes = CustomerChanges {
        name: Some("Ana López  ".into()),
        email: Patch::Set("Ana@Example.com".into()),
        ..Default::default()
    };
    let (customer, fields) = changes.apply(before.clone(), now() + chrono::Duration::hours(1)).unwrap();
    assert!(fields.is_empty());
    assert_eq!(customer, before);
}

#[test]
fn invalid_changes_are_refused() {
    assert!(CustomerChanges { name: Some(" ".into()), ..Default::default() }.apply(ana(), now()).is_err());
    assert!(CustomerChanges { email: Patch::Set("nope".into()), ..Default::default() }.apply(ana(), now()).is_err());
    assert!(CustomerChanges { notes: Patch::Set("x".repeat(2001)), ..Default::default() }.apply(ana(), now()).is_err());
}
