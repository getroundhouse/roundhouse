//! Hiding, showing and widening the two sidebars: the workspaces on the left, and the changes on the right. How wide
//! each is fitted, dragged and kept is `desktop::sides`' rule, which its unit tests pin, so these show that a click or
//! a drag reaches a sidebar, and that what's drawn beside one goes by it.

#![cfg(test)]

use client_core::fake_hub::World;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Pixels, SharedString, TestAppContext, point, px, size};
use protocol::model::{SessionStatus, WorkspaceId};

use crate::support::{FakeHub, Ui};

const LEFT_EDGE: &str = "left-sidebar-edge";
const RIGHT_EDGE: &str = "right-sidebar-edge";
/// How wide each sidebar is until it's dragged, as `docs/spec/sidebar.md` says.
const LEFT_USUALLY: Pixels = px(290.);
const RIGHT_USUALLY: Pixels = px(432.);

/// A window on a repository with two workspaces, the first showing, and both sidebars open, as they are from the
/// start. What's chosen for a sidebar stays in the app, and these ask nothing of a hub, so a fake one will do, with no
/// worker or agent to wait for. It goes with the test, so it's returned to be kept, with the workspace not showing.
fn with_two_workspaces(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, WorkspaceId) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspaces = ["ledger", "points"].map(|name| world.workspace(&repo, name));
    for workspace in &workspaces {
        world.chat(workspace, SessionStatus::Idle, Vec::new());
    }
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the sidebar of changes", |ui| ui.has(RIGHT_EDGE));
    let showing = ui.view().selected().cloned().unwrap();
    let other = workspaces.into_iter().find(|w| *w != showing).unwrap();
    (fake, ui, other)
}

/// Shows `workspace` by its row in the sidebar.
fn show(ui: &mut Ui, workspace: &WorkspaceId) {
    ui.click(SharedString::from(format!("workspace-{workspace}")));
    ui.wait("the workspace", |ui| ui.view().selected() == Some(workspace) && ui.has(RIGHT_EDGE));
}

fn window_width(ui: &mut Ui) -> Pixels {
    ui.cx.update_window(ui.handle, |_, window, _| window.viewport_size().width).unwrap()
}

/// How wide the sidebar on the left is drawn, which starts at the window's left edge.
fn left_shown(ui: &mut Ui) -> Pixels {
    ui.bounds(LEFT_EDGE).right()
}

/// How wide the sidebar of changes is drawn, which ends at the window's right edge.
fn right_shown(ui: &mut Ui) -> Pixels {
    window_width(ui) - ui.bounds(RIGHT_EDGE).left()
}

/// Presses on a sidebar's edge, halfway down it, and lets go with the pointer `to` across the window.
fn drag_edge(ui: &mut Ui, edge: &'static str, to: Pixels) {
    let down = ui.bounds(edge).center().y;
    drag_edge_at(ui, edge, down, to);
}

/// The same, pressing on the edge `down` from the window's top.
fn drag_edge_at(ui: &mut Ui, edge: &'static str, down: Pixels, to: Pixels) {
    let from = point(ui.bounds(edge).center().x, down);
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
pub(crate) fn show_an_error(ui: &mut Ui) {
    let view = ui.view.clone();
    ui.cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.error = Some("Couldn't fetch".to_string());
            cx.notify();
        });
    });
    ui.frame();
}

/// The tests run under Reduce Motion, which skips the slide, so each sidebar goes and comes back at once. What's beside
/// the one on the left, a workspace or Home, has its room while it's hidden, and starts after the button. `Cmd-B` does
/// what the button does, and the View menu says which it'll do.
#[gpui_kit::test]
fn the_button_beside_the_windows_buttons_hides_the_sidebar_and_shows_it_again(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = with_two_workspaces(cx);
    assert!(ui.has("add-project") && ui.has(LEFT_EDGE));
    let button = ui.bounds("toggle-sidebar");
    let title = ui.bounds("workspace-title");
    ui.input("half");

    ui.click("toggle-sidebar");

    assert!(!ui.has("add-project") && !ui.has(LEFT_EDGE), "none of the sidebar is drawn");
    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed", "the composer still has the keys");
    assert_eq!(ui.bounds("toggle-sidebar"), button, "the button stays where it was");
    let moved = ui.bounds("workspace-title");
    assert!(moved.left() > button.right(), "the header starts after the button: {moved:?}");
    assert!(moved.left() < title.left(), "and the workspace has the sidebar's room: {moved:?}");
    assert_eq!(crate::menus::items(&ui, "View")[0], "Show Sidebar");

    ui.press("secondary-b");

    assert!(ui.has("add-project"), "Cmd-B shows it again");
    assert_eq!(crate::menus::items(&ui, "View")[0], "Hide Sidebar");
    assert_eq!(ui.bounds("workspace-title"), title);
    ui.click("toggle-diff");
    assert!(!ui.has(RIGHT_EDGE), "the sidebar of changes goes at once too");

    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
    let search = ui.bounds("home-search");
    assert_eq!(search.left(), LEFT_USUALLY + px(16.), "beside the sidebar, Home starts where a header does");

    ui.click("toggle-sidebar");

    let search = ui.bounds("home-search");
    assert_eq!(search.left(), button.right() + px(16.), "Home's search starts after the button: {search:?}");
    assert!(ui.has("home-page") && !ui.has("add-project"));
}

/// Each edge runs up through the strip along the window's top, where a press that nothing takes moves the window, so
/// it's dragged there as well as halfway down. The test window panics if anything asks it to move. Settings stands in
/// for the sidebar on the left at the width it was dragged to.
#[gpui_kit::test]
fn a_sidebar_is_as_wide_as_its_edge_was_dragged_to_in_every_workspace_and_in_settings(cx: &mut TestAppContext) {
    let (_hub, mut ui, other) = with_two_workspaces(cx);
    assert_eq!(left_shown(&mut ui), LEFT_USUALLY);
    assert_eq!(right_shown(&mut ui), RIGHT_USUALLY);

    drag_edge(&mut ui, LEFT_EDGE, px(350.));
    let to = window_width(&mut ui) - px(500.);
    drag_edge(&mut ui, RIGHT_EDGE, to);

    assert_eq!(left_shown(&mut ui), px(350.));
    assert_eq!(right_shown(&mut ui), px(500.));
    assert_eq!(ui.bounds("workspace-title").left(), px(350. + 16.), "the workspace starts where it ends");

    let in_the_strip = px(20.);
    drag_edge_at(&mut ui, LEFT_EDGE, in_the_strip, px(360.));
    let to = window_width(&mut ui) - px(510.);
    drag_edge_at(&mut ui, RIGHT_EDGE, in_the_strip, to);

    let dragged = (px(360.), px(510.));
    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), dragged, "dragged in the strip along the top");

    ui.click("toggle-sidebar");
    ui.click("toggle-sidebar");
    show(&mut ui, &other);

    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), dragged, "hidden, shown again and in another workspace");

    ui.press("secondary-,");

    assert_eq!(ui.bounds("settings-back").right(), px(360. - 12.), "Settings has Back at its top right");
}

/// What's beside a sidebar moves with it, and the header stays clear of the window's buttons all the way.
#[gpui_kit::test]
fn a_sidebar_slides_shut_and_whats_beside_it_moves_with_it(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = with_two_workspaces(cx);
    ui.allow_motion();
    let button = ui.bounds("toggle-sidebar");

    ui.click("toggle-sidebar");
    ui.click("toggle-diff");

    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (LEFT_USUALLY, RIGHT_USUALLY), "each starts where it was");
    assert_eq!(ui.bounds("workspace-title").left(), LEFT_USUALLY + px(16.));
    slide_on(&mut ui, 0.5);

    let (left, right) = (left_shown(&mut ui), right_shown(&mut ui));
    assert!(px(0.) < left && left < LEFT_USUALLY, "partway shut: {left:?}");
    assert!(px(0.) < right && right < RIGHT_USUALLY, "partway shut: {right:?}");
    let tab = ui.bounds("sidebar-tab-changes");
    assert!(tab.right() > window_width(&mut ui), "the rest of it is past the window's edge: {tab:?}");
    // The window draws the sidebar and the workspace draws its header, each by how far the sidebar has slid.
    let title = ui.bounds("workspace-title").left();
    assert_eq!(title, left.max(button.right()) + px(16.), "the header starts where the sidebar ends, or the button");
    let across = window_width(&mut ui);
    assert_eq!(ui.bounds("toggle-diff").right(), across - right - px(12.), "and ends where the other starts");

    slide_on(&mut ui, 0.6);
    assert!(!ui.has(LEFT_EDGE) && !ui.has(RIGHT_EDGE), "shut, they're not drawn");
    assert_eq!(ui.bounds("workspace-title").left(), button.right() + px(16.));
    // The composer moved with the sidebar, so the frame it arrived on reports where its caret went on the next.
    ui.bring_the_next_frame();
    assert_eq!(slide_on(&mut ui, 0.1), 0, "and nothing asks for another frame");
}

#[gpui_kit::test]
fn a_sidebar_turned_back_partway_slides_from_where_it_had_got_to(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = with_two_workspaces(cx);
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

/// A workspace shown partway through a slide finds the sidebar where it is, rather than where it's going.
#[gpui_kit::test]
fn a_workspace_shown_partway_through_a_slide_starts_its_header_where_the_sidebar_is(cx: &mut TestAppContext) {
    let (_hub, mut ui, other) = with_two_workspaces(cx);
    let first = ui.view().selected().cloned().unwrap();
    show(&mut ui, &other);
    ui.allow_motion();
    ui.click("toggle-sidebar");
    slide_on(&mut ui, 0.25);

    let view = ui.view.clone();
    ui.cx.update(|cx| view.update(cx, |view, cx| view.select(first, cx)));

    let (left, title) = (left_shown(&mut ui), ui.bounds("workspace-title").left());
    assert!(left > px(102.), "the sidebar still covers the button: {left:?}");
    assert_eq!(title, left + px(16.));
}

/// A sidebar gives up its room where it isn't drawn, as the sidebar of changes isn't on Home, and as it slides away,
/// and takes it as it slides back. Fitted by which is open and which hidden, the one sliding took its whole width and
/// the other all the room, in the frame the slide set off.
#[gpui_kit::test]
fn in_a_narrow_window_a_sidebar_leaves_its_room_where_its_not_drawn_and_as_it_slides_away(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = with_two_workspaces(cx);
    let workspace = ui.view().selected().cloned().unwrap();
    narrow_the_window(&mut ui);
    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (px(240.), px(300.)));

    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));

    assert_eq!(left_shown(&mut ui), LEFT_USUALLY, "on Home, where no changes show");

    show(&mut ui, &workspace);
    assert_eq!((left_shown(&mut ui), right_shown(&mut ui)), (px(240.), px(300.)), "back in the workspace");
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

/// An error sits above the header, in the strip beside the window's buttons, so the header is below them and starts
/// where it would with nothing to clear. While the sidebar is hidden, its button is over the error, where a click
/// dismisses the error, and a click on the button only shows the sidebar.
#[gpui_kit::test]
fn under_an_error_the_header_starts_at_the_windows_edge_and_the_button_leaves_the_error_alone(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = with_two_workspaces(cx);
    ui.click("toggle-sidebar");
    let button = ui.bounds("toggle-sidebar");
    assert_eq!(ui.bounds("workspace-title").left(), button.right() + px(16.));

    show_an_error(&mut ui);

    let title = ui.bounds("workspace-title");
    assert!(title.top() >= button.bottom(), "the header is below the button: {title:?}");
    assert_eq!(title.left(), px(16.));
    // The error is in the strip, so it's what clears the window's buttons.
    let error = ui.bounds("error-text");
    assert!(error.left() >= button.right(), "the error starts after the button: {error:?}");

    ui.click("toggle-sidebar");

    assert!(ui.has(LEFT_EDGE), "the sidebar shows again");
    assert!(ui.view().error.is_some(), "and the error is still there");
}
