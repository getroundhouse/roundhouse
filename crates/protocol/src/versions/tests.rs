use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::Versions;

fn versions() -> impl Strategy<Value = Versions> {
    // Small numbers, so ranges often overlap, and some backwards, which speak nothing.
    (0..8_u32, 0..8_u32).prop_map(|(oldest, newest)| Versions { oldest, newest })
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// The one chosen is the newest that both speak, and there's none when they share none, whichever side asks.
    #[test]
    fn the_newest_version_both_sides_speak_is_chosen(ours in versions(), theirs in versions()) {
        let both = (0..8).filter(|&version| ours.speaks(version) && theirs.speaks(version)).max();
        prop_assert_eq!(ours.newest_shared(theirs), both);
        prop_assert_eq!(theirs.newest_shared(ours), both);
    }
}

#[test]
fn a_hello_from_before_ranges_speaks_only_its_version() {
    assert_eq!(Versions::from_hello(3, None), Versions::only(3));
    assert_eq!(Versions::from_hello(4, Some(3)), Versions { oldest: 3, newest: 4 });
}

#[test]
fn a_refusal_says_which_versions_a_side_speaks() {
    assert_eq!(Versions::only(3).to_string(), "protocol 3");
    assert_eq!(Versions { oldest: 3, newest: 4 }.to_string(), "protocols 3 to 4");
    assert_eq!(Versions { oldest: 5, newest: 4 }.to_string(), "protocols 5 to 4", "a backwards range shows as one");
}
