// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:21.765080
//+ Revised: 	2026/11/14 12:05:21.765080
// -----------------------------------------------------------------------------

//! The address of the product's key page.

use super::*;

#[test]
fn the_key_page_address_names_the_product_and_stays_offline() {
    let computer = Computer::new();
    let client = computer.start();
    assert_eq!(client.get_key_url(), format!("http://fake/apply?product={PRODUCT}"));
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0, "nothing was sent");
}

#[test]
fn the_key_page_address_encodes_the_product_code() {
    let computer = Computer::new();
    let config = Config::new("avalio lite&x=ñ", FakeServer::trusted_keys(), "2.0.0");
    let client = Client::new(
        config,
        Box::new(computer.store.clone()),
        Box::new(computer.server.clone()),
        Box::new(computer.clock.clone()),
    );
    assert_eq!(client.get_key_url(), "http://fake/apply?product=avalio%20lite%26x%3D%C3%B1");
}
