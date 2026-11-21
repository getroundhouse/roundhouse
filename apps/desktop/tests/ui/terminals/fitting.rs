//! The size a terminal's view gives it as the room beside a sliding sidebar changes. Which size a view gives, and when,
//! is `terminal_view`'s rule, which its unit tests pin, so this shows that a sidebar's slide reaches it.

use gpui_kit::{AppContext as _, TestAppContext};
use look::motion::PANEL;
use protocol::terminal::TerminalSize;

use super::selecting::with_a_made_up_terminal;
use crate::support::Ui;

/// The size the terminal in front last had from its view.
fn given(ui: &Ui<'_>) -> Option<TerminalSize> {
    let terminal = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.terminal()).unwrap();
    ui.cx.read_entity(&terminal, |t, _| t.given_size())
}

/// How many whole cells fit where the last frame drew the terminal in front.
fn room(ui: &mut Ui<'_>) -> TerminalSize {
    let bounds = ui.bounds(terminal_view::SHOWN_ID);
    let terminal = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.terminal()).unwrap();
    let cell = ui.cx.read_entity(&terminal, |t, _| t.cell_size());
    TerminalSize { cols: whole(bounds.size.width / cell.width), rows: whole(bounds.size.height / cell.height) }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a window's worth of cells")]
fn whole(cells: f32) -> u16 {
    cells.floor() as u16
}

/// A sidebar sliding beside a terminal leaves it at the size it has, with its grid cut off or its room left empty,
/// and the terminal takes the size it has room for once the slide ends, so its program is resized once rather than on
/// every frame. Each sidebar is hidden, which widens the room, then shown again, which narrows it: the one on the
/// left, which slides wherever a workspace shows, and the changes, which slide only where they're drawn.
#[gpui_kit::test]
fn a_terminal_takes_the_size_it_has_room_for_once_a_sidebar_beside_it_stops_sliding(cx: &mut TestAppContext) {
    let (_hub, mut ui) = with_a_made_up_terminal(cx);
    let before = room(&mut ui);
    assert_eq!(given(&ui), Some(before), "it has the size it has room for");
    ui.allow_motion();

    for (toggle, sidebar) in [("toggle-sidebar", "the workspaces"), ("toggle-diff", "the changes")] {
        let hidden = slide(&mut ui, toggle, &format!("hiding {sidebar}"));
        assert!(hidden.cols > before.cols, "hiding {sidebar} widened the room: {hidden:?}");
        let shown = slide(&mut ui, toggle, &format!("showing {sidebar}"));
        assert_eq!(shown, before, "showing {sidebar} gave back the size it had");
    }
}

/// Clicks `toggle`, which slides a sidebar, and checks that halfway through the slide the terminal still has the size
/// it had, though its room has changed by many columns, and that once it ends the terminal has the size its room has,
/// which this returns.
fn slide(ui: &mut Ui<'_>, toggle: &'static str, case: &str) -> TerminalSize {
    let before = given(ui).unwrap();

    ui.click(toggle);
    ui.play(PANEL.mul_f32(0.5));

    let partway = room(ui);
    assert!(partway.cols.abs_diff(before.cols) > 10, "{case}: the room has changed by many columns: {partway:?}");
    assert_eq!(given(ui), Some(before), "{case}: it keeps its size partway");
    ui.play(PANEL.mul_f32(0.6));
    let after = room(ui);
    assert_eq!(given(ui), Some(after), "{case}: once the slide ends, it has the size it has room for");
    after
}
