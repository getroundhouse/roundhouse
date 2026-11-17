//! Dragging a project's row in the sidebar to another place among the projects, which takes its workspaces with it,
//! and scrolls the list on near either end. The order a drag leaves is the app's own, kept in its settings, and nothing
//! here asks anything of a hub, so these open on a fake one.

#![cfg(test)]

use std::path::Path;
use std::time::Duration;

use desktop::app::AppView;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta,
    TestAppContext, point, px, size,
};
use protocol::model::{RepoId, WorkspaceId};

use crate::support::{
    FakeHub, MORE_THAN_FIT, Ui, open_on_the_world, project_row, sidebar_row as row, top, two_projects_world,
    with_a_chat,
};

/// Presses in the middle of `from`, and lets go with the pointer in the middle of `to`.
fn drag(ui: &mut Ui, from: ElementId, to: ElementId) {
    let (from, to) = (ui.bounds(from).center(), ui.bounds(to).center());
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.frame();
}

fn press(ui: &mut Ui, at: Point<Pixels>) {
    ui.send(MouseDownEvent { button: MouseButton::Left, position: at, click_count: 1, ..Default::default() });
}

/// Moves the pointer to `to` with the left button held.
fn hold_at(ui: &mut Ui, to: Point<Pixels>) {
    ui.send(MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), ..Default::default() });
}

fn let_go(ui: &mut Ui, at: Point<Pixels>) {
    ui.send(MouseUpEvent { button: MouseButton::Left, position: at, click_count: 1, ..Default::default() });
}

/// The projects the app's settings, saved in `settings`, put in order, from the top.
fn saved_order(settings: &Path) -> Vec<String> {
    let saved = std::fs::read_to_string(settings).unwrap_or_default();
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap_or_default();
    let order = saved["project_order"].as_array().cloned().unwrap_or_default();
    order.iter().map(|id| id.as_str().unwrap().to_string()).collect()
}

/// The window on two projects of two workspaces each, as [`two_projects_world`] makes them. The hub goes with the
/// test, so it's returned to be kept.
fn two_projects(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, [RepoId; 2], [WorkspaceId; 4]) {
    let (world, projects, four) = two_projects_world();
    let (fake, mut ui) = open_on_the_world(cx, world);
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == 4);
    (fake, ui, projects, four)
}

/// How much shorter [`one_long`] makes the window, so the sidebar's list is some 440 points tall rather than 660.
const SHORTER: Pixels = px(220.);

/// [`two_projects`], with the first or second as `long` says given [`MORE_THAN_FIT`] more workspaces, and the newest
/// of those, which is at the top of its project. In a window [`SHORTER`] than usual, so the long project is over twice
/// as tall as what shows of the list. So a project held near the top of the list, once it's scrolled to its end, is
/// still over the long one's lower half. And one held near either end has a long way to scroll.
fn one_long(cx: &mut TestAppContext, long: usize) -> (FakeHub, Ui<'_>, [RepoId; 2], [WorkspaceId; 4], WorkspaceId) {
    let (mut world, projects, four) = two_projects_world();
    let made = Vec::from_fn(MORE_THAN_FIT, |i| with_a_chat(&mut world, &projects[long], &format!("more-{i}")));
    let (fake, mut ui) = open_on_the_world(cx, world);
    let was = ui.cx.update_window(ui.handle, |_, window, _| window.bounds().size).unwrap();
    ui.cx.simulate_window_resize(ui.handle, size(was.width, was.height - SHORTER));
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == MORE_THAN_FIT + 4);
    (fake, ui, projects, four, made.last().unwrap().clone())
}

/// A drag that doesn't take the project past the other's middle leaves everything as it was, and is no press. One
/// past it moves the project, and its workspaces, and the order is saved.
#[gpui_kit::test]
fn a_project_dragged_past_the_others_middle_moves_with_its_workspaces_and_one_dragged_less_changes_nothing(
    cx: &mut TestAppContext,
) {
    let (fake, mut ui, [first, second], [upper, _, third, last]) = two_projects(cx);
    let was = top(&mut ui, project_row(&second));

    // A little way down its own project, and let go.
    drag(&mut ui, project_row(&first), row(&upper));

    assert_eq!(top(&mut ui, project_row(&second)), was, "dragged within its own project, it's where it was");
    assert!(ui.has(row(&upper)), "dragged within its own project: a drag isn't a press, so it isn't collapsed");
    assert!(
        ui.cx.read(AppView::saved_project_order).is_empty(),
        "dragged within its own project: nor is an order saved"
    );
    ui.click(project_row(&first));
    assert!(!ui.has(row(&upper)), "dragged within its own project: a press collapses it, as before");
    ui.click(project_row(&first));
    assert!(ui.has(row(&upper)), "dragged within its own project: and another brings it back");

    // Past the middle of the second project, which is among its workspaces.
    drag(&mut ui, project_row(&first), row(&last));

    assert!(
        top(&mut ui, project_row(&second)) < top(&mut ui, project_row(&first)),
        "dragged past: it's after the other"
    );
    assert!(top(&mut ui, row(&third)) < top(&mut ui, row(&upper)), "dragged past: with its workspaces under it");
    let saved = [second.to_string(), first.to_string()];
    ui.wait("the order the drag past left to be saved", |_| saved_order(&fake.settings_file()) == saved);
    ui.click(row(&last));
    ui.press("secondary-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&upper), "dragged past: and the keys go in the order shown");
    // And back up past the other's middle.
    drag(&mut ui, project_row(&first), project_row(&second));
    assert!(top(&mut ui, project_row(&first)) < top(&mut ui, project_row(&second)), "dragged back: where it was");
}

#[gpui_kit::test]
fn a_dragged_projects_row_and_workspaces_follow_the_pointer_as_one_and_slide_to_its_place_once_let_go(
    cx: &mut TestAppContext,
) {
    let (_fake, mut ui, [first, second], [upper, lower, _, last]) = two_projects(cx);
    let (row_was, upper_was, lower_was) =
        (top(&mut ui, project_row(&first)), top(&mut ui, row(&upper)), top(&mut ui, row(&lower)));
    // Its place after the other is as far up from where the list ends as its lines reach.
    let place = ui.bounds(row(&last)).bottom() - (ui.bounds(row(&lower)).bottom() - row_was);
    ui.allow_motion();
    let from = ui.bounds(project_row(&first)).center();
    press(&mut ui, from);

    // The drag starts once the pointer has moved a little way, and the row follows it from there, so it doesn't jump.
    // Then down past the middle of the other project, which is 58px below where this one's row starts without it.
    let started = from + point(px(0.), px(10.));
    hold_at(&mut ui, started);
    hold_at(&mut ui, started + point(px(0.), px(60.)));

    assert_eq!(top(&mut ui, project_row(&first)), row_was + px(60.), "its row is where the pointer has it");
    assert_eq!(top(&mut ui, row(&upper)), upper_was + px(60.), "and its workspaces go with it");
    assert_eq!(top(&mut ui, row(&lower)), lower_was + px(60.));
    ui.settle(look::motion::PANEL / 4);
    assert_eq!(top(&mut ui, project_row(&second)), row_was, "the other has slid up into its place");
    assert_eq!(top(&mut ui, project_row(&first)), row_was + px(60.), "while it stays with the pointer");
    assert!(ui.view().dragging_a_project());

    let_go(&mut ui, started + point(px(0.), px(60.)));

    assert_eq!(top(&mut ui, project_row(&first)), row_was + px(60.), "let go, it starts from where it was left");
    ui.play(look::motion::PANEL / 2);
    let halfway = top(&mut ui, project_row(&first));
    assert!(row_was + px(60.) < halfway && halfway < place, "{halfway:?} isn't on its way to {place:?}");
    ui.settle(look::motion::PANEL / 4);
    assert_eq!(top(&mut ui, project_row(&first)), place, "and comes to rest there");
    assert_eq!(top(&mut ui, row(&upper)) - place, upper_was - row_was, "with its workspaces under it");
    assert!(!ui.view().dragging_a_project(), "and once it's there, it's no longer over the others");
    assert!(ui.has(row(&upper)), "a drag isn't a press, so it isn't collapsed");
}

/// A project held near the bottom of the list scrolls it on, past a project too long to show whole, and goes with the
/// pointer, so it can go past one whose middle was out of sight. The list stops at its end.
#[gpui_kit::test]
fn a_project_held_near_the_bottom_of_the_list_scrolls_it_on_as_far_as_its_end(cx: &mut TestAppContext) {
    let (fake, mut ui, [first, second], [_, lower, .., last_of_all], _) = one_long(cx, 1);
    assert!(!ui.has(row(&last_of_all)), "the second project's last workspace is below the window");
    let (area, from) = (ui.bounds("workspaces-list"), ui.bounds(project_row(&first)).center());
    press(&mut ui, from);
    hold_at(&mut ui, from + point(px(0.), px(10.)));

    let near_the_bottom = point(from.x, area.bottom() - px(8.));
    hold_at(&mut ui, near_the_bottom);

    let was = top(&mut ui, project_row(&second));
    ui.play(Duration::from_millis(50));
    ui.play(Duration::from_millis(50));
    let scrolled = top(&mut ui, project_row(&second));
    assert!(scrolled < was, "the list has scrolled up under it, {scrolled:?} from {was:?}");
    ui.settle(Duration::from_millis(50));
    assert!(ui.has(row(&last_of_all)), "as far as its end");
    assert!(top(&mut ui, row(&last_of_all)) < top(&mut ui, project_row(&first)), "and it has gone past the second");
    let end = ui.bounds(row(&lower)).bottom();
    assert_eq!(end, area.bottom(), "where it goes no further than the end of the projects, which is in sight");
    let_go(&mut ui, near_the_bottom);
    let saved = [second.to_string(), first.to_string()];
    ui.wait("the order to be saved", |_| saved_order(&fake.settings_file()) == saved);
}

/// And near the top, back up, as far as the list's top, where it goes no further than the first project's place.
#[gpui_kit::test]
fn a_project_held_near_the_top_of_the_list_scrolls_it_back_as_far_as_its_top(cx: &mut TestAppContext) {
    let (fake, mut ui, [first, second], [_, lower, ..], newest) = one_long(cx, 0);
    let first_was = top(&mut ui, project_row(&first));
    ui.scroll(row(&newest), ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    assert!(!ui.has(project_row(&first)), "the first project's row is above the window");
    let (area, from) = (ui.bounds("workspaces-list"), ui.bounds(project_row(&second)).center());
    press(&mut ui, from);
    hold_at(&mut ui, from - point(px(0.), px(10.)));

    let near_the_top = point(from.x, area.top() + px(8.));
    hold_at(&mut ui, near_the_top);

    // The first project's last workspace, just above it.
    let was = top(&mut ui, row(&lower));
    ui.play(Duration::from_millis(50));
    ui.play(Duration::from_millis(50));
    let scrolled = top(&mut ui, row(&lower));
    assert!(scrolled > was, "the list has scrolled down under it, {scrolled:?} from {was:?}");
    ui.settle(Duration::from_millis(50));
    assert_eq!(top(&mut ui, project_row(&second)), first_was, "as far as its top, and the first project's place");
    assert!(top(&mut ui, project_row(&second)) < top(&mut ui, project_row(&first)), "which it has gone past");
    let_go(&mut ui, near_the_top);
    let saved = [second.to_string(), first.to_string()];
    ui.wait("the order to be saved", |_| saved_order(&fake.settings_file()) == saved);
}

/// The surface under a dragged project is a place of its own in the list, so it shows wherever any of the project's
/// lines do, even once its row is out of sight.
#[gpui_kit::test]
fn a_project_held_past_the_top_of_the_list_is_on_its_surface_with_its_row_out_of_sight(cx: &mut TestAppContext) {
    let (_fake, mut ui, [_, second], [.., third, _], newest) = one_long(cx, 0);
    ui.scroll(row(&newest), ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    let (area, from) = (ui.bounds("workspaces-list"), ui.bounds(project_row(&second)).center());
    press(&mut ui, from);
    hold_at(&mut ui, from - point(px(0.), px(10.)));
    assert!(ui.has("dragged-project"), "it's on its surface");

    hold_at(&mut ui, point(from.x, area.top() - px(40.)));

    assert!(!ui.has(project_row(&second)), "its row is above what's in sight");
    assert!(ui.has(row(&third)), "and its first workspace isn't");
    assert!(ui.has("dragged-project"), "which is still on its surface");
}

/// The kit ends a drag as the button comes up, and the app draws the frame after. That frame lets go of the project
/// before anything else, so the list doesn't scroll on once more, by however long that frame took to come.
#[gpui_kit::test]
fn a_project_let_go_while_the_list_scrolls_on_under_it_stays_where_it_was_let_go(cx: &mut TestAppContext) {
    let (_fake, mut ui, [first, second], _, newest) = one_long(cx, 1);
    let (area, from) = (ui.bounds("workspaces-list"), ui.bounds(project_row(&first)).center());
    press(&mut ui, from);
    hold_at(&mut ui, from + point(px(0.), px(10.)));
    let near_the_bottom = point(from.x, area.bottom() - px(8.));
    hold_at(&mut ui, near_the_bottom);
    ui.play(Duration::from_millis(50));
    ui.play(Duration::from_millis(50));
    let scrolled = top(&mut ui, row(&newest));

    ui.cx.executor().advance_clock(Duration::from_millis(50));
    let_go(&mut ui, near_the_bottom);

    assert_eq!(top(&mut ui, row(&newest)), scrolled, "the list scrolls no further once it's let go");
    ui.settle(look::motion::PANEL / 4);
    assert_eq!(top(&mut ui, row(&newest)), scrolled, "nor after");
    assert!(top(&mut ui, project_row(&first)) < top(&mut ui, project_row(&second)), "it hadn't got past the other");
}
