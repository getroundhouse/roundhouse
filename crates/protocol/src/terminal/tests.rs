use super::TerminalSize;

#[test]
fn a_worker_gives_a_terminal_any_size_from_one_cell_to_the_most() {
    let size = |cols, rows| TerminalSize { cols, rows }.is_allowed();

    assert!(size(1, 1) && size(80, 24) && TerminalSize::MOST.is_allowed());
    assert!(!size(0, 24) && !size(80, 0));
    assert!(!size(TerminalSize::MOST.cols + 1, 24) && !size(80, TerminalSize::MOST.rows + 1));
}
