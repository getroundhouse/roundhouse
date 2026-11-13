use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

// Through one of the types the macro makes, which tests it for all of them.
use crate::terminal::TerminalBytes;

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Any bytes at all, since what a terminal's program writes needn't be text, and a read can end in the middle of a
    /// character.
    #[test]
    fn bytes_come_back_as_they_went(bytes in vec(any::<u8>(), 0..512)) {
        let sent = TerminalBytes::new(bytes.clone());

        let line = crate::frame::encode(&sent).unwrap();

        prop_assert_eq!(crate::frame::decode::<TerminalBytes>(&line).unwrap().into_vec(), bytes);
    }
}

#[test]
fn bytes_travel_as_base64() {
    assert_eq!(crate::frame::encode(&TerminalBytes::new(b"hi".to_vec())).unwrap(), b"\"aGk=\"\n");
    assert!(crate::frame::decode::<TerminalBytes>(b"\"not base64!\"").is_err());
}

/// A frame's debug output is logged, and a burst of output would bury the rest of the line.
#[test]
fn bytes_show_only_how_many_there_are() {
    assert_eq!(format!("{:?}", TerminalBytes::new(vec![b'x'; 4096])), "TerminalBytes(4096 bytes)");
}
