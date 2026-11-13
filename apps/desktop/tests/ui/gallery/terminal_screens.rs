//! The screens about a workspace's terminals: the menu the `+` after the tabs opens, and a terminal in front, in the
//! chat's place.

#![cfg(test)]

use protocol::terminal::TerminalSize;

use super::frame::{TABS_MENU, TERMINAL};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;
use super::worlds::{conversation, converse, sidebar};

/// The screens about terminals.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "new-tab-menu",
        spec: "chats.md",
        world: Some(conversation),
        frame: TABS_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("new-tab"), Step::MoveAway],
    },
    Screen {
        name: "terminal",
        spec: "chats.md",
        world: Some(with_a_terminal),
        frame: TERMINAL,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("terminal-term-1"),
            Step::WaitFor(terminal_view::SHOWN_ID),
            Step::MoveAway,
        ],
    },
];

/// The size the screen's terminal is, which the gallery's hub doesn't let the view change.
const SIZE: TerminalSize = TerminalSize { cols: 100, rows: 16 };

/// A test run in the screen's workspace, with colours, and the prompt after it.
const TEST_RUN: &[u8] = b"\x1b[1;32maspen\x1b[0m $ cargo test -p hub\r\n\
\x20  \x1b[1;32mCompiling\x1b[0m hub v0.1.0 (/workspaces/aspen/crates/hub)\r\n\
\x20   \x1b[1;32mFinished\x1b[0m `test` profile [unoptimized + debuginfo] target(s) in 4.21s\r\n\
\x20    \x1b[1;32mRunning\x1b[0m unittests src/lib.rs\r\n\
\r\n\
running 3 tests\r\n\
test hub::terminals::tests::a_terminal_opens_in_the_worktree ... \x1b[32mok\x1b[0m\r\n\
test server::terminals::tests::whats_typed_reaches_the_worker_in_order ... \x1b[32mok\x1b[0m\r\n\
test store::tests::a_record_keeps_its_revision ... \x1b[32mok\x1b[0m\r\n\
\r\n\
test result: \x1b[32mok\x1b[0m. 3 passed; 0 failed; 0 ignored\r\n\
\r\n\
\x1b[1;32maspen\x1b[0m $ ";

/// The conversation, with a terminal open beside it that has run the tests.
fn with_a_terminal() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    converse(&mut world, &focus);
    world.terminal(&focus, SIZE, TEST_RUN);
    world
}
