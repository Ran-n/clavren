// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.161065
//+ Revised: 	2026/11/03 04:54:11.161065
// -----------------------------------------------------------------------------

//! Client library for Breren applications: activates a licence key, keeps the device key in the
//! operating system's secure storage, validates the cached activation token offline and renews
//! it in the background, and reports a typed outcome the application turns into its own
//! messages. See `docs/spec.md`, section 5, and `docs/client-integration.md`.
//!
//! ```no_run
//! use std::sync::Arc;
//! use clavren_client::{Background, Client, Config, Outcome, trusted_keys};
//!
//! let keys = trusted_keys(&[("v1", "<public key from clavren-admin show-public-key>")]).unwrap();
//! let config = Config::new("avalio-lite", keys, env!("CARGO_PKG_VERSION"));
//! let client = Arc::new(Client::for_windows(config, "https://licences.example.com", "Breren"));
//! match client.status() {
//!     Outcome::Licensed { .. } => { /* run */ }
//!     Outcome::NeedsActivation => { /* ask for a key, then client.activate(&key) */ }
//!     _ => { /* show the matching message */ }
//! }
//! let checks = Background::spawn(client.clone(), |outcome| { /* tell the UI thread */ }).unwrap();
//! ```

mod background;
mod client;
pub mod clock;
#[cfg(windows)]
mod dpapi;
mod online;
mod outcome;
mod state;
pub mod store;
pub mod transport;

use clavren_protocol::encoding;
use clavren_protocol::token::TrustedKeys;
use ed25519_dalek::VerifyingKey;

pub use background::{BACKOFF, Background};
pub use client::{Client, Config, ROLLBACK_TOLERANCE};
#[cfg(windows)]
pub use dpapi::Dpapi;
pub use outcome::{Outcome, Temporary};

/// The trusted token keys from `(kid, base64url public key)` pairs, as `clavren-admin
/// show-public-key` prints them. `None` if a key is malformed.
pub fn trusted_keys(keys: &[(&str, &str)]) -> Option<TrustedKeys> {
    keys.iter()
        .map(|(kid, key)| {
            let bytes = encoding::decode_array::<32>(key)?;
            Some((kid.to_string(), VerifyingKey::from_bytes(&bytes).ok()?))
        })
        .collect()
}

#[cfg(windows)]
impl Client {
    /// The usual setup on Windows: state in `%APPDATA%\<vendor>\<product>\licence.dat`, protected
    /// with DPAPI for the current user, and HTTPS to `server_url`.
    pub fn for_windows(config: Config, server_url: &str, vendor: &str) -> Self {
        let base = std::env::var_os("APPDATA").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let path = base.join(vendor).join(&config.product).join("licence.dat");
        let store = store::FileStore::new(path, Box::new(Dpapi::new(&config.product)));
        Self::new(
            config,
            Box::new(store),
            Box::new(transport::HttpTransport::new(server_url)),
            Box::new(clock::SystemClock),
        )
    }
}
