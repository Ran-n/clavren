// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:18:55.550022
//+ Revised: 	2026/11/03 04:54:17.806225
// -----------------------------------------------------------------------------

//! Everything the Clavren server and its clients must agree on: wire types, error codes, the
//! licence-key format and its normalization, the signed challenge message, and the activation
//! token claims. Both sides depend on this crate so the protocol has one definition and the two
//! can't drift apart. See `docs/spec.md`, sections 4 and 6, for the formats.

pub mod check;
pub mod crockford;
pub mod encoding;
pub mod licence_key;
pub mod token;
pub mod wire;
