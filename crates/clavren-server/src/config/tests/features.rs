// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:59:41.747635
//+ Revised: 	2026/11/14 11:59:41.747635
// -----------------------------------------------------------------------------

//! The settings of single features: updates, the application form's human check, token days
//! and the currency.

use super::*;

#[test]
fn updates_are_checked_and_live_in_the_data_dir_by_default() {
    let config = with(&[(DATA_DIR_VAR, "/srv/clavren")]).unwrap();
    assert_eq!(config.updates.dir, PathBuf::from("/srv/clavren/updates"));
    assert!(config.updates.checking);
    assert_eq!(config.updates.installer, UpdateSettings::default().installer);
    assert_eq!(config.updates.status, PathBuf::from("/var/lib/clavren-update/status.json"));
    assert_eq!(with(&[]).unwrap().updates.dir, UpdateSettings::default().dir);
}

#[test]
fn update_checks_can_be_turned_off_or_pointed_elsewhere() {
    assert!(!with(&[(UPDATE_SOURCE_VAR, "off")]).unwrap().updates.checking);
    assert!(!with(&[(UPDATE_SOURCE_VAR, " OFF ")]).unwrap().updates.checking);
    let forgejo = "https://codeberg.org/api/v1/repos/Ran-n/clavren/releases/latest";
    assert!(with(&[(UPDATE_SOURCE_VAR, forgejo)]).unwrap().updates.checking);
    let config = with(&[(UPDATE_SCRIPT_VAR, "/tmp/updater.sh"), (UPDATE_STATUS_VAR, "/tmp/status.json")]).unwrap();
    assert_eq!(config.updates.installer, PathBuf::from("/tmp/updater.sh"));
    assert_eq!(config.updates.status, PathBuf::from("/tmp/status.json"));
}

#[test]
fn an_unusable_update_source_is_refused() {
    for value in
        ["no", "ftp://example.com/release", "http://example.com/release", "https://", "https://example.com/a b"]
    {
        assert_invalid(with(&[(UPDATE_SOURCE_VAR, value)]), UPDATE_SOURCE_VAR);
    }
}

#[test]
fn the_application_form_has_no_human_check_by_default() {
    assert!(with(&[]).unwrap().applications.turnstile.is_none());
}

#[test]
fn turnstile_takes_both_keys() {
    let config = with(&[(TURNSTILE_SITE_KEY_VAR, " 0x4AAA "), (TURNSTILE_SECRET_VAR, "0x4BBB")]).unwrap();
    let turnstile = config.applications.turnstile.unwrap();
    assert_eq!((turnstile.site_key.as_str(), turnstile.secret.as_str()), ("0x4AAA", "0x4BBB"));
    assert_eq!(turnstile.verify_url, DEFAULT_TURNSTILE_VERIFY_URL);
    assert!(!format!("{turnstile:?}").contains("0x4BBB"), "the secret is never printed");
}

#[test]
fn turnstile_refuses_one_key_without_the_other() {
    assert_invalid(with(&[(TURNSTILE_SITE_KEY_VAR, "0x4AAA")]), TURNSTILE_SECRET_VAR);
    assert_invalid(with(&[(TURNSTILE_SECRET_VAR, "0x4BBB")]), TURNSTILE_SITE_KEY_VAR);
}

#[test]
fn turnstile_answers_are_verified_over_https_or_on_this_computer() {
    let keys = [(TURNSTILE_SITE_KEY_VAR, "a"), (TURNSTILE_SECRET_VAR, "b")];
    for url in ["https://verify.example.com/siteverify", "http://127.0.0.1:9000/siteverify", "http://localhost/x"] {
        let config = with(&[keys[0], keys[1], (TURNSTILE_VERIFY_URL_VAR, url)]).unwrap();
        assert_eq!(config.applications.turnstile.unwrap().verify_url, url);
    }
    for url in ["http://verify.example.com/siteverify", "ftp://x", "https://"] {
        assert_invalid(with(&[keys[0], keys[1], (TURNSTILE_VERIFY_URL_VAR, url)]), TURNSTILE_VERIFY_URL_VAR);
    }
}

#[test]
fn token_days_are_what_new_products_start_with() {
    assert_eq!(with(&[]).unwrap().licensing.new_product_offline_days(), 30);
    assert_eq!(with(&[(TOKEN_DAYS_VAR, "7")]).unwrap().licensing.new_product_offline_days(), 7);
    assert_invalid(with(&[(TOKEN_DAYS_VAR, "1")]), TOKEN_DAYS_VAR);
    assert_invalid(with(&[(TOKEN_DAYS_VAR, "3651")]), TOKEN_DAYS_VAR);
}

#[test]
fn the_default_currency_is_eur_unless_set() {
    assert_eq!(with(&[]).unwrap().licensing.default_currency.as_str(), "EUR");
    assert_eq!(with(&[(CURRENCY_VAR, "USD")]).unwrap().licensing.default_currency.as_str(), "USD");
    for bad in ["eur", "EURO", "€"] {
        assert_invalid(with(&[(CURRENCY_VAR, bad)]), CURRENCY_VAR);
    }
}
