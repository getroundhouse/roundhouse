//! Hiding, showing and widening the two sidebars: the workspaces on the left, and the changes on the right.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Pixels, TestAppContext, point, px, size};

use crate::support::{Stack, Ui};

const LEFT_EDGE: &str = "left-sidebar-edge";
const RIGHT_EDGE: &str = "right-sidebar-edge";
/// How wide each sidebar is until it's dragged, as `docs/spec/sidebar.md` says.
const LEFT_USUALLY: Pixels = px(290.);
const RIGHT_USUALLY: Pixels = px(432.);

/// A window with a workspace showing, and both sidebars open, as they are from the start.
fn with_a_workspace<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    ui.new_workspace();
    ui.wait("the sidebar of changes", |ui| ui.has(RIGHT_EDGE));
    ui
}

/// Where the last frame drew it.
fn bounds(ui: &mut Ui, id: &'static str) -> Bounds<Pixels> {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
}

fn window_width(ui: &mut Ui) -> Pixels {
    ui.cx.update_window(ui.handle, |_, window, _| window.viewport_size().width).unwrap()
}

/// How wide the sidebar on the left is drawn, which starts at the window's left edge.
fn left_shown(ui: &mut Ui) -> Pixels {
    bounds(ui, LEFT_EDGE).right()
}

/// How wide the sidebar of changes is drawn, which ends at the window's right edge.
fn right_shown(ui: &mut Ui) -> Pixels {
    window_width(ui) - bounds(ui, RIGHT_EDGE).left()
}

/// Presses on a sidebar's edge, halfway down it, and lets go with the pointer `to` across the window.
fn drag_edge(ui: &mut Ui, edge: &'static str, to: Pixels) {
    let down = bounds(ui, edge).center().y;
    drag_edge_at(ui, edge, down, to);
}

/// The same, pressing on the edge `down` from the window's top.
fn drag_edge_at(ui: &mut Ui, edge: &'static str, down: Pixels, to: Pixels) {
    let from = point(bounds(ui, edge).center().x, down);
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, point(to, down), cx)).unwrap();
    ui.frame();
}

/// Makes the window 900px wide, which is too narrow for both sidebars at their usual widths with room between them.
/// The one on the right gives way to its narrowest, 300px, and the one on the left to 240px.
fn narrow_the_window(ui: &mut Ui) {
    ui.cx.simulate_window_resize(ui.handle, size(px(900.), px(800.)));
    ui.frame();
}

/// Plays `part` of a slide's time.
fn slide_on(ui: &mut Ui, part: f32) -> usize {
    ui.play(look::motion::PANEL.mul_f32(part))
}

/// Puts an error in the strip along the top of what's beside the sidebar.
fn show_an_error(ui: &mut Ui) {
    let view = ui.view.clone();
    ui.cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.error = Some("Couldn't fetch".to_string());
            cx.notify();
        });
    });
    ui.frame();
}

#[gpui_kit::test]
fn the_button_beside_the_windows_buttons_hides_the_sidebar_and_shows_it_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    assert!(ui.has("add-project") && ui.has(LEFT_EDGE));
    let button = bounds(&mut ui, "toggle-sidebar");
    let title = bounds(&mut ui, "workspace-title");
    ui.input("half");

    ui.click("toggle-sidebar");

    assert!(!ui.has("add-project") && !ui.has(LEFT_EDGE), "none of the sidebar is drawn");
    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed", "the composer still has the keys");
    assert_eq!(bounds(&mut ui, "toggle-sidebar"), button, "the button stays where it was");
    let moved = bounds(&mut ui, "workspace-title");
    assert!(moved.left() > button.right(), "the header starts after the button: {moved:?}");
    assert!(moved.left() < title.left(), "and the workspace has the sidebar's room: {moved:?}");

    ui.click("toggle-sidebar");

    assert!(ui.has("add-project"));
    assert_eq!(bounds(&mut ui, "workspace-title"), title);
}

#[gpui_kit::test]
fn home_starts_after_the_button_while_the_sidebar_is_hidden(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
    let button = bounds(&mut ui, "toggle-sidebar");
    let search = bounds(&mut ui, "home-search");
    assert_eq!(search.left(), LEFT_USUALLY + px(16.), "beside the sidebar, it starts where a header does");

    ui.click("toggle-sidebar");

    let search = bounds(&mut ui, "home-search");
    assert_eq!(search.left(), button.right() + px(16.), "the search starts after the button: {search:?}");
    assert!(ui.has("home-page") && !ui.has("add-project"));
}

#[gpui_kit::test]
fn dragging_a_sidebars_edge_changes_how_wide_it_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    assert_eq!(left_shown(&mut ui), LEFT_USUALLY);
    assert_eq!(right_shown(&mut ui), RIGHT_USUALLY);

    drag_edge(&mut ui, LEFT_EDGE, px(350.));
    let to = window_width(&mut ui) - px(500.);
    drag_edge(&mut ui, RIGHT_EDGE, to);

    assert_eq!(left_shown(&mut ui), px(350.));
    assert_eq!(right_shown(&mut ui), px(500.));
    assert_eq!(bounds(&mut ui, "workspace-title").left(), px(350. + 16.), "the workspace starts where it ends");
}

/// Each edge runs up through the strip along the window's top, where a press that nothing takes moves the window. The
/// test window panics if anything asks it to move.
#[gpui_kit::test]
fn dragging_an_edge_in_the_strip_along_the_top_widens_the_sidebar_and_leaves_the_window(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    let in_the_strip = px(20.);

    drag_edge_at(&mut ui, LEFT_EDGE, in_the_strip, px(350.));
    let to = window_width(&mut ui) - px(500.);
    drag_edge_at(&mut ui, RIGHT_EDGE, in_the_strip, to);

    assert_eq!(left_shown(&mut ui), px(350.));
    assert_eq!(right_shown(&mut ui), px(500.));
}

#[gpui_kit::test]
fn a_sidebar_dragged_close_to_its_usual_width_takes_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    drag_edge(&mut ui, LEFT_EDGE, px(350.));
    let to = window_width(&mut ui) - px(500.);
    drag_edge(&mut ui, RIGHT_EDGE, to);

    drag_edge(&mut ui, LEFT_EDGE, px(296.));
    let to = window_width(&mut ui) - px(427.);
    drag_edge(&mut ui, RIGHT_EDGE, to);

    assert_eq!(left_shown(&mut ui), LEFT_USUALLY);
    assert_eq!(right_shown(&mut ui), RIGHT_USUALLY);
}

#[gpui_kit::test]
fn a_sidebar_keeps_its_width_while_its_hidden_and_in_another_workspace(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    drag_edge(&mut ui, LEFT_EDGE, px(350.));
    let to = window_width(&mut ui) - px(500.);
    drag_edge(&mut ui, RIGHT_EDGE, to);

    ui.click("toggle-sidebar");
    ui.click("toggle-sidebar");
    ui.new_workspace();
    ui.wait("the sidebar of changes", |ui| ui.has(RIGHT_EDGE));

    assert_eq!(left_shown(&mut ui), px(350.));
    assert_eq!(right_shown(&mut ui), px(500.));
}

/// What's beside a sidebar moves with it, and the header stays clear of the window's buttons all the way.
#[gpui_kit::test]
fn a_sidebar_slides_shut_and_whats_beside_it_moves_with_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    ui.allow_motion();
    let button = bounds(&mut ui, "toggle-sidebar");

    ui.click("toggle-sidebar");
    ui.click("toggle-diff");

    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (LEFT_USUALLY, RIGHT_USUALLY), "each starts where it was");
    assert_eq!(bounds(&mut ui, "workspace-title").left(), LEFT_USUALLY + px(16.));
    slide_on(&mut ui, 0.5);

    let (left, right) = (left_shown(&mut ui), right_shown(&mut ui));
    assert!(px(0.) < left && left < LEFT_USUALLY, "partway shut: {left:?}");
    assert!(px(0.) < right && right < RIGHT_USUALLY, "partway shut: {right:?}");
    let tab = bounds(&mut ui, "sidebar-tab-changes");
    assert!(tab.right() > window_width(&mut ui), "the rest of it is past the window's edge: {tab:?}");
    // The window draws the sidebar and the workspace draws its header, each by how far the sidebar has slid.
    let title = bounds(&mut ui, "workspace-title").left();
    assert_eq!(title, left.max(button.right()) + px(16.), "the header starts where the sidebar ends, or the button");
    let across = window_width(&mut ui);
    assert_eq!(bounds(&mut ui, "toggle-diff").right(), across - right - px(12.), "and ends where the other starts");

    slide_on(&mut ui, 0.6);
    assert!(!ui.has(LEFT_EDGE) && !ui.has(RIGHT_EDGE), "shut, they're not drawn");
    assert_eq!(bounds(&mut ui, "workspace-title").left(), button.right() + px(16.));
    assert_eq!(slide_on(&mut ui, 0.1), 0, "and nothing asks for another frame");
}

#[gpui_kit::test]
fn a_sidebar_turned_back_partway_slides_from_where_it_had_got_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    ui.click("toggle-sidebar");
    ui.allow_motion();

    ui.click("toggle-sidebar");

    assert!(!ui.has(LEFT_EDGE), "none of it shows yet");
    slide_on(&mut ui, 0.5);
    let partway = left_shown(&mut ui);
    assert!(px(0.) < partway && partway < LEFT_USUALLY, "partway open: {partway:?}");

    ui.click("toggle-sidebar");
    assert_eq!(left_shown(&mut ui), partway);
    ui.settle(look::motion::PANEL);
    assert!(!ui.has(LEFT_EDGE));
}

/// A workspace shown for the first time partway through a slide finds the sidebar where it is, rather than where it's
/// going.
#[gpui_kit::test]
fn a_workspace_shown_partway_through_a_slide_starts_its_header_where_the_sidebar_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    let first = ui.view().selected.clone().unwrap();
    ui.new_workspace();
    ui.allow_motion();
    ui.click("toggle-sidebar");
    slide_on(&mut ui, 0.25);

    let view = ui.view.clone();
    ui.cx.update(|cx| view.update(cx, |view, cx| view.select(first, cx)));

    let (left, title) = (left_shown(&mut ui), bounds(&mut ui, "workspace-title").left());
    assert!(left > px(102.), "the sidebar still covers the button: {left:?}");
    assert_eq!(title, left + px(16.));
}

#[gpui_kit::test]
fn reduce_motion_skips_the_slide(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);

    ui.click("toggle-sidebar");
    ui.click("toggle-diff");

    assert!(!ui.has(LEFT_EDGE) && !ui.has(RIGHT_EDGE));
}

/// A sidebar gives up its room as it slides away, and takes it as it slides back. Fitted by which is open and which
/// hidden, the one sliding took its whole width and the other all the room, in the frame the slide set off.
#[gpui_kit::test]
fn in_a_narrow_window_a_sidebar_sliding_away_leaves_its_room_as_it_goes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    narrow_the_window(&mut ui);
    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (px(240.), px(300.)));
    ui.allow_motion();

    ui.click("toggle-sidebar");

    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (px(240.), px(300.)), "nothing has moved yet");
    // A slide slows as it arrives, so a tenth of its time takes it about a quarter of the way.
    slide_on(&mut ui, 0.1);
    let (left, right) = (left_shown(&mut ui), right_shown(&mut ui));
    assert!(left < px(240.) && px(300.) < right && right < RIGHT_USUALLY, "partway: {left:?} and {right:?}");
    assert!(px(900.) - left - right >= px(359.), "with the room between them all the way");
    slide_on(&mut ui, 1.);
    assert_eq!(right_shown(&mut ui), RIGHT_USUALLY, "and the one on the right has its usual width once it's gone");
}

/// Dragged narrower in a narrow window, the left sidebar gives the right one its room, and takes it back.
#[gpui_kit::test]
fn an_edge_dragged_narrower_and_back_gets_back_the_width_it_had(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    ui.cx.simulate_window_resize(ui.handle, size(px(1000.), px(800.)));
    ui.frame();
    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (LEFT_USUALLY, px(350.)));

    drag_edge(&mut ui, LEFT_EDGE, px(250.));
    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (px(250.), px(390.)));
    drag_edge(&mut ui, LEFT_EDGE, LEFT_USUALLY);

    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (LEFT_USUALLY, px(350.)));
}

/// The sidebar of changes takes room only where it's drawn, so Home has the sidebar at the width it was dragged to.
#[gpui_kit::test]
fn in_a_narrow_window_the_sidebar_has_its_width_where_no_changes_show(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    narrow_the_window(&mut ui);
    assert_eq!(left_shown(&mut ui), px(240.));

    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));

    assert_eq!(left_shown(&mut ui), LEFT_USUALLY);
}

/// An error sits above the header, in the strip beside the window's buttons, so the header is below them and starts
/// where it would with nothing to clear.
#[gpui_kit::test]
fn under_an_error_the_header_starts_at_the_windows_edge_while_the_sidebar_is_hidden(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    ui.click("toggle-sidebar");
    let button = bounds(&mut ui, "toggle-sidebar");
    assert_eq!(bounds(&mut ui, "workspace-title").left(), button.right() + px(16.));

    show_an_error(&mut ui);

    let title = bounds(&mut ui, "workspace-title");
    assert!(title.top() >= button.bottom(), "the header is below the button: {title:?}");
    assert_eq!(title.left(), px(16.));
    // The error is in the strip, so it's what clears the window's buttons.
    let error = bounds(&mut ui, "error-text");
    assert!(error.left() >= button.right(), "the error starts after the button: {error:?}");
}

/// While the sidebar is hidden, its button is over whatever is in the strip along the top, and a click on an error
/// there dismisses it.
#[gpui_kit::test]
fn the_button_leaves_an_error_under_it_alone(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    ui.click("toggle-sidebar");
    show_an_error(&mut ui);

    ui.click("toggle-sidebar");

    assert!(ui.has(LEFT_EDGE), "the sidebar shows again");
    assert!(ui.view().error.is_some(), "and the error is still there");
}

/// Each can be dragged wide while the other is hidden, to more than the window has room for with both showing.
#[gpui_kit::test]
fn sidebars_too_wide_for_the_window_together_leave_room_between_them(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    let across = window_width(&mut ui);
    ui.click("toggle-diff");
    drag_edge(&mut ui, LEFT_EDGE, px(480.));
    ui.click("toggle-sidebar");
    ui.click("toggle-diff");
    drag_edge(&mut ui, RIGHT_EDGE, across - px(800.));
    assert_eq!(right_shown(&mut ui), px(800.));

    ui.click("toggle-sidebar");

    let (left, right) = (left_shown(&mut ui), right_shown(&mut ui));
    assert_eq!(left, px(480.), "the one on the left keeps its width");
    assert_eq!(across - left - right, px(360.), "and the one on the right gives way: {right:?}");
}

#[gpui_kit::test]
fn settings_stands_in_for_the_sidebar_at_the_width_it_was_dragged_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_workspace(cx, &stack);
    drag_edge(&mut ui, LEFT_EDGE, px(350.));

    ui.press("cmd-,");

    assert_eq!(bounds(&mut ui, "settings-back").right(), px(350. - 12.), "Back is at its top right");
}
