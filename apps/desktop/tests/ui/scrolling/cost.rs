//! How few of a long session's rows a frame draws: the ones that show, and a few more whose heights are out of date.

use desktop::workspace_pane::WorkspacePane;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ScrollDelta, TestAppContext, point, px};

use super::{A_FRAME, open_on_a_long_session, session_saying, wait_for_the_tail};
use crate::support::{Stack, Ui};

/// A change of the chat's width puts every row's height out of date. Only the rows in the window, and a few to a
/// frame of those out of sight, are measured again, where every row once was in that one frame.
#[gpui_kit::test]
fn a_change_of_width_measures_what_shows_and_not_the_whole_session(cx: &mut TestAppContext) {
    /// More than a window holds with what's laid out past its ends, and far fewer than are loaded.
    const A_WINDOWFUL: u64 = 150;
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    let transcript = ui.transcript().unwrap();
    let drawn = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().rows_drawn());
    let to_measure = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().rows_to_measure());
    let loaded = ui.cx.read_entity(&transcript, |t, _| t.list().loaded_rows());
    assert!(u64::try_from(loaded).unwrap() > 2 * A_WINDOWFUL, "only {loaded} loaded");
    assert_eq!(to_measure(&ui), 0, "at rest, every loaded row has its height");
    let before = drawn(&ui);

    // Hides the sidebar of changes, which gives the chat its room, and draws the one frame that asks for.
    let pane = ui.pane().unwrap();
    ui.cx.update_entity(&pane, WorkspacePane::toggle_changes);
    ui.frame();

    let in_that_frame = drawn(&ui) - before;
    assert!(in_that_frame > 0 && in_that_frame < A_WINDOWFUL, "{in_that_frame} of {loaded} rows drawn at once");
    let left = to_measure(&ui);
    assert!(left > loaded / 2 && left < loaded, "{left} of {loaded} rows are left to measure after one frame");
    // The rest are measured over the frames after, and then there's nothing more to draw.
    ui.settle(A_FRAME);
    assert_eq!(to_measure(&ui), 0);
    assert_eq!(ui.play(A_FRAME), 0);
}

/// A frame of a transcript at rest draws the rows in its window, and none of the rest to find heights it has.
#[gpui_kit::test]
fn a_frame_at_rest_draws_only_the_rows_that_show(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Replies of a line each, so a window holds a few dozen of them and the room past its ends holds as many again.
    let newest = session_saying(&stack, 300, |i| format!("message {i}"));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let total = wait_for_the_tail(&mut ui, &newest);
    // Away from the end, so there are rows past both ends of the window.
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(2000.))));
    ui.settle(A_FRAME);
    let window = ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap();
    let top = ui.cx.read_entity(&transcript, |t, _| t.list().top());
    // The rows from the one at the window's top down to its bottom, but for those that show nothing.
    let showing_something = ui.cx.read_entity(&transcript, |t, _| {
        let (mut row_top, mut showing) = (-top.offset, 0);
        for row in top.row..total {
            if row_top >= window.size.height {
                break;
            }
            let height = t.list().row_height(row).unwrap();
            showing += usize::from(height > px(0.));
            row_top += height;
        }
        showing
    });
    assert!(showing_something > 5, "only {showing_something} rows show");
    let drawn = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().rows_drawn());
    let before = drawn(&ui);

    ui.cx.update_entity(&transcript, |_, cx| cx.notify());
    ui.frame();

    assert_eq!(drawn(&ui) - before, u64::try_from(showing_something).unwrap());
}

/// What the agent adds comes at the end, and alters no row before it, so nothing else is measured again for it. A
/// page of history that loads or is let go does put every height out of date, and a new row was once taken for one.
#[gpui_kit::test]
fn a_row_the_agent_adds_puts_no_other_rows_height_out_of_date(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    let transcript = ui.transcript().unwrap();
    let to_measure = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().rows_to_measure());
    assert_eq!(to_measure(&ui), 0, "at rest, every loaded row has its height");
    let (session, rows) = ui.cx.read_entity(&transcript, |t, _| (t.session_id().clone(), t.row_count()));

    stack.run(&session, "say:one more thing");
    ui.wait("the reply", |ui| ui.cx.read_entity(&transcript, |t, _| t.row_count() > rows));

    // Each new row is drawn as it comes, since the transcript keeps to the latest row.
    assert_eq!(to_measure(&ui), 0, "rows put out of date by what the agent added");
}
