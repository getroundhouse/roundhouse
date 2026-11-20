use super::{HeldUpdates, JOINED_OUTPUT_BYTES, TerminalSize, TerminalUpdate};

#[test]
fn a_worker_gives_a_terminal_any_size_from_one_cell_to_the_most() {
    let size = |cols, rows| TerminalSize { cols, rows }.is_allowed();

    assert!(size(1, 1) && size(80, 24) && TerminalSize::MOST.is_allowed());
    assert!(!size(0, 24) && !size(80, 0));
    assert!(!size(TerminalSize::MOST.cols + 1, 24) && !size(80, TerminalSize::MOST.rows + 1));
}

fn output(bytes: &[u8]) -> TerminalUpdate {
    TerminalUpdate::Output { output: bytes.into() }
}

/// Up to a joined update's most, and only onto output, so a new size or the end stays in its place among them, and
/// not across a fence.
#[test]
fn held_output_joins_onto_output_up_to_the_most_an_update_carries() {
    let mut held = HeldUpdates::default();
    let most = vec![b'x'; JOINED_OUTPUT_BYTES - 2];
    let resized = TerminalUpdate::Resized { size: TerminalSize { cols: 80, rows: 24 } };

    let places = [
        held.hold(output(b"a")),
        held.hold(output(b"b")),
        held.hold(output(&most)),
        held.hold(output(b"c")),
        held.hold(resized.clone()),
        held.hold(output(b"d")),
        held.hold(output(b"e")),
    ];
    held.fence();
    let fenced = held.hold(output(b"f"));
    let output_bytes = held.output_bytes();

    assert_eq!(places, [true, false, false, true, true, true, false]);
    assert!(fenced, "it joined across the fence");
    assert_eq!(output_bytes, JOINED_OUTPUT_BYTES + 4);
    let sent: Vec<_> = std::iter::from_fn(|| held.take_oldest()).collect();
    let joined = output(&[b"ab".as_slice(), &most].concat());
    assert_eq!(sent, [joined, output(b"c"), resized, output(b"de"), output(b"f")]);
    assert_eq!(held.output_bytes(), 0);
}
