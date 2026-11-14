// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:48:57.716892
//+ Revised: 	2026/11/14 11:48:57.716892
// -----------------------------------------------------------------------------

//! What an application tells the client about itself and its licence server.

use clavren_protocol::token::TrustedKeys;
use std::time::Duration;

pub struct Config {
    /// The product code, as in Clavren.
    pub product: String,
    /// The token-signing public keys this build trusts (`clavren-admin show-public-key`).
    pub trusted_keys: TrustedKeys,
    pub app_version: String,
    /// Shown to the administrator, e.g. the computer's name.
    pub device_name: String,
    pub platform: String,
    /// Check online once this long has passed since the last successful check.
    pub check_interval: Duration,
    /// While a licence application waits, ask how it went this often.
    pub application_check_interval: Duration,
}

impl Config {
    /// Checks daily (every 15 minutes while an application waits); names the device after the
    /// computer.
    pub fn new(product: &str, trusted_keys: TrustedKeys, app_version: &str) -> Self {
        Self {
            product: product.to_string(),
            trusted_keys,
            app_version: app_version.to_string(),
            device_name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "computer".into()),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            check_interval: Duration::from_secs(24 * 3600),
            application_check_interval: Duration::from_secs(15 * 60),
        }
    }
}
