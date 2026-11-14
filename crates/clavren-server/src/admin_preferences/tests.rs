// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:30.151414
//+ Revised: 	2026/11/14 11:36:30.151414
// -----------------------------------------------------------------------------

use super::*;

fn with_table(table: Table) -> Preferences {
    Preferences {
        version: VERSION,
        tables: BTreeMap::from([("licences".to_string(), table)]),
        ..Preferences::default()
    }
}

#[test]
fn sensible_preferences_pass() {
    let table = Table {
        filters: BTreeMap::from([("state".into(), "unused".into())]),
        search: Some("López".into()),
        order: Some("expiry".into()),
        page_size: Some(50),
        columns: Some(vec!["key".into(), "customer".into()]),
        issue: Some(IssueDraft {
            open: true,
            product: Some(Uuid::nil()),
            quantity: Some(1000),
            expires_at: NaiveDate::from_ymd_opt(2027, 12, 31),
        }),
    };
    let preferences = Preferences { theme: Theme::Dark, last_page: Some("/licences".into()), ..with_table(table) };
    assert!(preferences.check().is_ok());
}

#[test]
fn keys_are_never_remembered() {
    let search = with_table(Table { search: Some("brn 79yn 44xq w025 377z".into()), ..Table::default() });
    assert!(search.check().is_err());
    let filter = with_table(Table {
        filters: BTreeMap::from([("q".into(), "BRN-79YN-44XQ-W025-377Z".into())]),
        ..Table::default()
    });
    assert!(filter.check().is_err());
}

#[test]
fn odd_preferences_are_refused() {
    let refused = [
        Preferences { version: 2, ..Preferences::default() },
        Preferences { version: VERSION, last_page: Some("https://evil.example".into()), ..Preferences::default() },
        with_table(Table { page_size: Some(1000), ..Table::default() }),
        with_table(Table { search: Some("x".repeat(201)), ..Table::default() }),
        with_table(Table { columns: Some(vec!["c".into(); 31]), ..Table::default() }),
        with_table(Table {
            issue: Some(IssueDraft { quantity: Some(0), ..IssueDraft::default() }),
            ..Table::default()
        }),
        with_table(Table {
            issue: Some(IssueDraft { quantity: Some(1001), ..IssueDraft::default() }),
            ..Table::default()
        }),
        Preferences {
            version: VERSION,
            tables: BTreeMap::from([(
                "customers".to_string(),
                Table { issue: Some(IssueDraft::default()), ..Table::default() },
            )]),
            ..Preferences::default()
        },
        Preferences {
            version: VERSION,
            tables: BTreeMap::from([("secrets".to_string(), Table::default())]),
            ..Preferences::default()
        },
    ];
    for preferences in refused {
        assert!(preferences.check().is_err(), "{preferences:?}");
    }
    assert!(serde_json::from_str::<Preferences>(r#"{"version":1,"colour":"red"}"#).is_err(), "unknown fields");
}
