//! What has to hold for any pairing link: a phone reads back exactly what the hub wrote, and nothing it's given can
//! make it panic.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::PairingLink;

/// Any name and code, including spaces, `+`, `&`, `%` and text that isn't ASCII, and hosts as a hub writes them.
fn link() -> impl Strategy<Value = PairingLink> {
    let host =
        ("[a-zA-Z0-9.-]{1,20}|[0-9]{1,3}(\\.[0-9]{1,3}){3}", 1u16..).prop_map(|(host, port)| format!("{host}:{port}"));
    (".{1,30}", vec(host, 1..5), "[a-zA-Z0-9_-]{43}", ".{1,30}")
        .prop_map(|(hub_name, hosts, fingerprint, code)| PairingLink { hub_name, hosts, fingerprint, code })
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn a_link_reads_back_as_it_was_written(link in link()) {
        prop_assert_eq!(PairingLink::parse(&link.to_string()), Ok(link));
    }

    #[test]
    fn anything_after_the_prefix_is_read_without_panicking(rest in ".{0,200}") {
        let _ = PairingLink::parse(&format!("roundhouse://pair?{rest}"));
    }
}
