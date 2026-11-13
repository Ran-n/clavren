// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:18:55.610808
//+ Revised: 	2026/11/12 23:57:44.574960
// -----------------------------------------------------------------------------

//! The Clavren licensing service: licence generation and storage, atomic activation, device
//! verification, token issuing, the admin API and dashboard, and the `clavren-admin` CLI.
//! Modules are organised by domain (licences, products, activations…); see `docs/spec.md`,
//! section 14.

pub mod activations;
pub mod admin_auth;
pub mod admin_cli;
pub mod admin_preferences;
pub mod applications;
pub mod audit;
pub mod backups;
pub mod clock;
pub mod config;
pub mod crypto;
pub mod customers;
pub mod dashboard;
pub mod database;
pub mod http;
pub mod licences;
pub mod logging;
pub mod maintenance;
pub mod money;
pub mod overview;
pub mod products;
pub mod startup;
pub mod updates;
