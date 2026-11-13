//! Dragging a project's row in the sidebar to another place among the projects, which takes its workspaces with it,
//! and scrolls the list on near either end.

#![cfg(test)]

use std::time::Duration;

use desktop::app::AppView;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta,
    TestAppContext, point, px,
};
use protocol::model::{RepoId, WorkspaceId};

use crate::support::{MORE_THAN_FIT, Stack, Ui, project_row, sidebar_row as row, top, two_projects, wait_out_setup};

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

/// The projects the settings saved in the stack's data directory put in order, from the top.
fn saved_order(stack: &Stack) -> Vec<String> {
    let saved = std::fs::read_to_string(stack.settings_file()).unwrap_or_default();
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap_or_default();
    let order = saved["project_order"].as_array().cloned().unwrap_or_default();
    order.iter().map(|id| id.as_str().unwrap().to_string()).collect()
}

/// [`two_projects`], with the first or second as `long` says given more workspaces than the window has room for, and
/// the newest of those, which is at the top of its project.
fn one_long<'a>(
    cx: &'a mut TestAppContext,
    stack: &Stack,
    long: usize,
) -> (Ui<'a>, [RepoId; 2], [WorkspaceId; 4], WorkspaceId) {
    let (mut ui, projects, four) = two_projects(cx, stack);
    let client = stack.client();
    let made: Vec<WorkspaceId> =
        (0..MORE_THAN_FIT).map(|_| client.workspace_create(&projects[long], None, None, None).unwrap().id).collect();
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == MORE_THAN_FIT + 4);
    wait_out_setup(&mut ui);
    (ui, projects, four, made.last().unwrap().clone())
}

#[gpui_kit::test]
fn dragging_a_projects_row_moves_it_and_its_workspaces_among_the_others(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], [upper, _, third, last]) = two_projects(cx, &stack);

    // Past the middle of the second project, which is among its workspaces.
    drag(&mut ui, project_row(&first), row(&last));

    assert!(top(&mut ui, project_row(&second)) < top(&mut ui, project_row(&first)), "it's after the other now");
    assert!(top(&mut ui, row(&third)) < top(&mut ui, row(&upper)), "with its workspaces under it");
    let saved = [second.to_string(), first.to_string()];
    ui.wait("the order to be saved", |_| saved_order(&stack) == saved);
    ui.click(row(&last));
    ui.press("secondary-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&upper), "and the keys go in the order shown");
    // And back up past the other's middle.
    drag(&mut ui, project_row(&first), project_row(&second));
    assert!(top(&mut ui, project_row(&first)) < top(&mut ui, project_row(&second)), "back where it was");
}

#[gpui_kit::test]
fn a_dragged_projects_row_and_workspaces_follow_the_pointer_as_one_and_slide_to_its_place_once_let_go(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second], [upper, lower, _, last]) = two_projects(cx, &stack);
    let (row_was, upper_was, lower_was) =
        (top(&mut ui, project_row(&first)), top(&mut ui, row(&upper)), top(&mut ui, row(&lower)));
    // Its place after the other is as far up from where the list ends as its lines reach.
    let place = ui.bounds(row(&last)).bottom() - (ui.bounds(row(&lower)).bottom() - row_was);
    ui.allow_motion();
    let from = ui.bounds(project_row(&first)).center();
    press(&mut ui, from);

    // The drag starts once the pointer has moved a little way, and the row follows it from there, so it doesn't jump.
    // Then down past the middle of the other project, which is 57px below where this one's row starts without it.
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
    let stack = Stack::new();
    let (mut ui, [first, second], [_, lower, .., last_of_all], _) = one_long(cx, &stack, 1);
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
    ui.wait("the order to be saved", |_| saved_order(&stack) == saved);
}

/// And near the top, back up, as far as the list's top, where it goes no further than the first project's place.
#[gpui_kit::test]
fn a_project_held_near_the_top_of_the_list_scrolls_it_back_as_far_as_its_top(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], [_, lower, ..], newest) = one_long(cx, &stack, 0);
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
    ui.wait("the order to be saved", |_| saved_order(&stack) == saved);
}

/// The surface under a dragged project is a place of its own in the list, so it shows wherever any of the project's
/// lines do, even once its row is out of sight.
#[gpui_kit::test]
fn a_project_held_past_the_top_of_the_list_is_on_its_surface_with_its_row_out_of_sight(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [_, second], [.., third, _], newest) = one_long(cx, &stack, 0);
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
    let stack = Stack::new();
    let (mut ui, [first, second], _, newest) = one_long(cx, &stack, 1);
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

#[gpui_kit::test]
fn a_drag_that_doesnt_take_a_project_past_another_changes_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], [upper, ..]) = two_projects(cx, &stack);
    let was = top(&mut ui, project_row(&second));

    // A little way down its own project, and let go.
    drag(&mut ui, project_row(&first), row(&upper));

    assert_eq!(top(&mut ui, project_row(&second)), was, "it's where it was");
    assert!(ui.has(row(&upper)), "and a drag isn't a press, so it isn't collapsed");
    assert!(ui.cx.read(AppView::saved_project_order).is_empty(), "nor is an order saved");
    ui.click(project_row(&first));
    assert!(!ui.has(row(&upper)), "a press collapses it, as before");
}
