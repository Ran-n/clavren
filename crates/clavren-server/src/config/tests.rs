// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.918280
//+ Revised: 	2026/11/09 11:10:33.412079
// -----------------------------------------------------------------------------

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use super::*;

fn lookup_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let values: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    move |name| values.get(name).cloned()
}

fn minimal() -> Vec<(&'static str, &'static str)> {
    vec![
        (DATABASE_URL_VAR, "postgres://clavren:secret@localhost:5432/clavren"),
        (PUBLIC_URL_VAR, "https://licences.example.com"),
    ]
}

fn with(overrides: &[(&'static str, &'static str)]) -> Result<Config, ConfigError> {
    let mut pairs = minimal();
    pairs.retain(|(name, _)| !overrides.iter().any(|(other, _)| other == name));
    pairs.extend_from_slice(overrides);
    Config::from_lookup(lookup_from(&pairs))
}

fn assert_invalid(result: Result<Config, ConfigError>, variable: &str) {
    match result {
        Err(ConfigError::Invalid { name, .. }) => assert_eq!(name, variable),
        other => panic!("expected {variable} to be rejected, got {other:?}"),
    }
}

#[test]
fn minimal_configuration_uses_the_defaults() {
    let config = with(&[]).unwrap();
    assert_eq!(config.listen_address, "127.0.0.1:8080".parse().unwrap());
    assert_eq!(config.data_dir, PathBuf::from("/var/lib/clavren"));
    assert_eq!(config.public_url, "https://licences.example.com");
    assert_eq!(config.proxies.trusted, ["127.0.0.1".parse::<IpAddr>().unwrap(), "::1".parse().unwrap()]);
    assert_eq!(config.proxies.client_ip_header, "cf-connecting-ip");
    assert_eq!(config.admin.session_idle, Duration::from_secs(12 * 3600));
    assert_eq!(config.admin.session_lifetime, Duration::from_secs(7 * 24 * 3600));
    assert_eq!((config.admin.login_max_failures, config.admin.login_failure_window), (5, Duration::from_secs(900)));
    assert!(config.admin.secure_cookies);
    assert_eq!(config.licensing.token_lifetime, Duration::from_secs(30 * 24 * 3600));
    assert_eq!(config.licensing.signing_key_version, 1);
}

#[test]
fn the_defaults_match_the_default_settings() {
    let config = with(&[]).unwrap();
    let (proxies, admin) = (ProxySettings::default(), AdminAuthSettings::default());
    assert_eq!(config.proxies.trusted, proxies.trusted);
    assert_eq!(config.proxies.client_ip_header, proxies.client_ip_header);
    assert_eq!(config.admin.session_idle, admin.session_idle);
    assert_eq!(config.admin.session_lifetime, admin.session_lifetime);
    assert_eq!(config.admin.login_max_failures, admin.login_max_failures);
    assert_eq!(config.admin.login_failure_window, admin.login_failure_window);
    let licensing = LicensingSettings::default();
    assert_eq!(config.licensing.token_lifetime, licensing.token_lifetime);
    assert_eq!(config.licensing.signing_key_version, licensing.signing_key_version);
}

#[test]
fn every_value_can_be_set() {
    let config = with(&[
        (LISTEN_VAR, "0.0.0.0:9000"),
        (DATA_DIR_VAR, "/srv/clavren"),
        (TRUSTED_PROXIES_VAR, "10.0.0.1, 10.0.0.2"),
        (CLIENT_IP_HEADER_VAR, "X-Forwarded-For"),
        (SESSION_IDLE_HOURS_VAR, "2"),
        (SESSION_MAX_DAYS_VAR, "1"),
        (LOGIN_MAX_FAILURES_VAR, "3"),
        (LOGIN_WINDOW_MINUTES_VAR, "60"),
        (TOKEN_DAYS_VAR, "7"),
        (SIGNING_KEY_VERSION_VAR, "2"),
    ])
    .unwrap();
    assert_eq!(config.listen_address, "0.0.0.0:9000".parse().unwrap());
    assert_eq!(config.data_dir, PathBuf::from("/srv/clavren"));
    assert_eq!(config.proxies.trusted.len(), 2);
    assert_eq!(config.proxies.client_ip_header, "x-forwarded-for");
    assert_eq!(config.admin.session_idle, Duration::from_secs(2 * 3600));
    assert_eq!(config.admin.session_lifetime, Duration::from_secs(24 * 3600));
    assert_eq!((config.admin.login_max_failures, config.admin.login_failure_window), (3, Duration::from_secs(3600)));
    assert_eq!(
        (config.licensing.token_lifetime, config.licensing.signing_key_version),
        (Duration::from_secs(7 * 24 * 3600), 2)
    );
}

#[test]
fn trusting_no_proxy_is_possible() {
    assert!(with(&[(TRUSTED_PROXIES_VAR, "none")]).unwrap().proxies.trusted.is_empty());
}

#[test]
fn missing_required_values_are_named() {
    assert_eq!(Config::from_lookup(lookup_from(&[])).unwrap_err(), ConfigError::Missing(DATABASE_URL_VAR));
    let only_database = lookup_from(&[(DATABASE_URL_VAR, "postgres://localhost/clavren")]);
    assert_eq!(Config::from_lookup(only_database).unwrap_err(), ConfigError::Missing(PUBLIC_URL_VAR));
}

#[test]
fn blank_values_count_as_missing() {
    assert_eq!(with(&[(PUBLIC_URL_VAR, "   ")]).unwrap_err(), ConfigError::Missing(PUBLIC_URL_VAR));
    assert_eq!(with(&[(LISTEN_VAR, "")]).unwrap().listen_address, "127.0.0.1:8080".parse().unwrap());
}

/// Regression: sqlx parsed a `mysql://` URL as PostgreSQL options without complaint, so the
/// mistake only surfaced as a confusing connection failure.
#[test]
fn a_database_url_for_another_database_is_rejected_without_showing_the_password() {
    for url in ["mysql://user:hunter2@localhost/db", "sqlite://hunter2.db", "hunter2"] {
        let error = with(&[(DATABASE_URL_VAR, url)]).unwrap_err();
        assert!(matches!(error, ConfigError::Invalid { name: DATABASE_URL_VAR, .. }), "{url} accepted");
        assert!(!error.to_string().contains("hunter2"));
    }
}

#[test]
fn both_postgres_url_schemes_are_accepted() {
    assert!(with(&[(DATABASE_URL_VAR, "postgresql://clavren@localhost/clavren")]).is_ok());
    assert!(with(&[(DATABASE_URL_VAR, " postgres://clavren@localhost:5433/clavren ")]).is_ok());
}

#[test]
fn an_invalid_listen_address_is_rejected() {
    assert_invalid(with(&[(LISTEN_VAR, "localhost")]), LISTEN_VAR);
}

#[test]
fn the_public_url_loses_its_trailing_slash() {
    let config = with(&[(PUBLIC_URL_VAR, "https://licences.example.com/")]).unwrap();
    assert_eq!(config.public_url, "https://licences.example.com");
}

#[test]
fn plain_http_is_only_allowed_for_localhost_and_turns_secure_cookies_off() {
    let local = with(&[(PUBLIC_URL_VAR, "http://localhost:8080")]).unwrap();
    assert!(!local.admin.secure_cookies);
    assert!(with(&[(PUBLIC_URL_VAR, "http://127.0.0.1")]).is_ok());
    assert_invalid(with(&[(PUBLIC_URL_VAR, "http://licences.example.com")]), PUBLIC_URL_VAR);
}

#[test]
fn the_public_url_must_be_scheme_and_host_only() {
    for value in ["licences.example.com", "https://", "https://example.com/path", "ftp://example.com"] {
        assert_invalid(with(&[(PUBLIC_URL_VAR, value)]), PUBLIC_URL_VAR);
    }
}

#[test]
fn invalid_proxy_settings_are_rejected() {
    assert_invalid(with(&[(TRUSTED_PROXIES_VAR, "127.0.0.1, cloudflared")]), TRUSTED_PROXIES_VAR);
    assert_invalid(with(&[(CLIENT_IP_HEADER_VAR, "not a header")]), CLIENT_IP_HEADER_VAR);
}

#[test]
fn durations_and_limits_must_be_whole_numbers_above_zero() {
    for (name, value) in [
        (SESSION_IDLE_HOURS_VAR, "0"),
        (SESSION_MAX_DAYS_VAR, "-1"),
        (LOGIN_MAX_FAILURES_VAR, "five"),
        (LOGIN_WINDOW_MINUTES_VAR, "1.5"),
        (TOKEN_DAYS_VAR, "0"),
        (SIGNING_KEY_VERSION_VAR, "v2"),
    ] {
        assert_invalid(with(&[(name, value)]), name);
    }
}

/// Regression: any length of time was accepted, and adding a huge one to today panicked the
/// request that made a session or a token.
#[test]
fn lengths_of_time_stop_at_ten_years() {
    for (name, longest) in [
        (SESSION_IDLE_HOURS_VAR, "87600"),
        (SESSION_MAX_DAYS_VAR, "3650"),
        (LOGIN_WINDOW_MINUTES_VAR, "5256000"),
        (TOKEN_DAYS_VAR, "3650"),
    ] {
        assert!(with(&[(name, longest)]).is_ok(), "{name}={longest}");
        let longer: &'static str = (longest.parse::<u64>().unwrap() + 1).to_string().leak();
        assert_invalid(with(&[(name, longer)]), name);
    }
    assert_invalid(with(&[(TOKEN_DAYS_VAR, "18446744073709551615")]), TOKEN_DAYS_VAR);
}

#[test]
fn debug_output_hides_the_database_password() {
    let printed = format!("{:?}", with(&[]).unwrap());
    assert!(!printed.contains("secret"), "{printed}");
    assert!(printed.contains("<redacted>"));
}

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
