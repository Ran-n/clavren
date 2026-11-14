// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:31.723510
//+ Revised: 	2026/11/14 11:36:31.723510
// -----------------------------------------------------------------------------

use super::*;

fn answer(retry_after: Option<u64>) -> Answer {
    Answer { status: 429, body: Value::Null, retry_after }
}

/// Regression: a huge Retry-After overflowed the checker's clock arithmetic and ended its
/// thread.
#[test]
fn the_wait_a_server_asks_for_is_at_most_a_day() {
    assert_eq!(retry_after(&answer(None)), Duration::from_secs(60));
    assert_eq!(retry_after(&answer(Some(90))), Duration::from_secs(90));
    assert_eq!(retry_after(&answer(Some(u64::MAX))), LONGEST_RETRY_AFTER);
}

#[test]
fn a_device_name_is_sent_as_the_server_takes_it() {
    assert_eq!(device_name("  OFFICE-PC "), "OFFICE-PC");
    assert_eq!(device_name(" "), UNNAMED_DEVICE);
    assert_eq!(device_name(&"ñ".repeat(300)).chars().count(), MAX_DEVICE_NAME);
}
