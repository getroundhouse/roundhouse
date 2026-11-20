//! What a follower holds for a slow client.

use protocol::terminal::{TerminalSize, TerminalUpdate};

use super::super::backlog::{BEHIND_UPDATES, Backlog, JOINED_OUTPUT_BYTES};
use super::stand_in::output;

/// Up to a frame's most, and only onto output, so a new size or the end stays in its place among them.
#[test]
fn held_output_joins_onto_output_up_to_the_most_a_frame_carries() {
    let mut held = Backlog::default();
    let most = "x".repeat(JOINED_OUTPUT_BYTES - 2);
    let resized = TerminalUpdate::Resized { size: TerminalSize { cols: 80, rows: 24 } };

    for update in [output("a"), output("b"), output(&most), output("c"), resized.clone(), output("d"), output("e")] {
        held.hold(update);
    }

    let sent: Vec<_> = std::iter::from_fn(|| held.take_oldest()).collect();
    assert_eq!(sent, [output(&format!("ab{most}")), output("c"), resized, output("de")]);
}

/// Past the most updates it holds, as well as past the most output, so small updates can't pile up without limit.
#[test]
fn a_client_is_too_far_behind_past_the_most_updates_held() {
    let mut held = Backlog::default();
    let resized = TerminalUpdate::Resized { size: TerminalSize { cols: 80, rows: 24 } };
    for _ in 0..BEHIND_UPDATES {
        held.hold(resized.clone());
    }
    let at_the_most = held.too_far_behind();

    held.hold(resized);

    assert!(!at_the_most);
    assert!(held.too_far_behind());
}
