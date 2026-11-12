//! A file's lines in the file tab: the lines git left out, which fold and open, how they're shown, and selecting
//! them to copy.

#![cfg(test)]

use std::fmt::Write as _;

use desktop::file_diff::{DiffView, FileDiff};
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, MouseButton, ScrollDelta, TestAppContext, point, px};

use crate::support::{Diff, Stack, Ui, git};

/// A workspace that changes line 30 of a file 40 lines long, which its base has, with the file's diff open.
fn changing_a_long_file<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, Diff) {
    let lines = (1..=40).map(|n| format!("line {n}\n")).collect::<Vec<_>>().concat();
    changing(cx, stack, "long.txt", &lines, ("line 30\n", "line thirty\n"))
}

/// A workspace that changes `file`, which its base has as `lines`, by replacing `change`'s first with its second, with
/// the file's diff open.
fn changing<'a>(
    cx: &'a mut TestAppContext,
    stack: &Stack,
    file: &str,
    lines: &str,
    change: (&str, &str),
) -> (Ui<'a>, Diff) {
    std::fs::write(stack.repo().join(file), lines).unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "long"]);
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    std::fs::write(ui.worktree(&workspace).join(file), lines.replacen(change.0, change.1, 1)).unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, file);
    (ui, diff)
}

/// The arrow on the fold of the lines above the first change, which opens the lines just above it.
fn up_at_the_top() -> ElementId {
    ("fold-up", 1_usize).into()
}

/// The arrow on the fold of the lines below the change, which opens the lines just below it.
fn down_at_the_end() -> ElementId {
    ("fold-down", 34_usize).into()
}

fn shown_as(ui: &Ui<'_>, diff: &Diff) -> DiffView {
    ui.cx.read_entity(&diff.file, FileDiff::shown_as)
}

#[gpui_kit::test]
fn the_lines_git_left_out_open_from_the_change_a_step_at_a_time(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = changing_a_long_file(cx, &stack);
    assert!(ui.cx.read_entity(&diff.file, |f, _| f.has_read_the_base()), "the file as it was fills in the folds");
    assert!(ui.has(up_at_the_top()) && ui.has(down_at_the_end()));

    ui.click(up_at_the_top());

    assert!(ui.has(up_at_the_top()), "twenty of the 26 lines above the change show, and the rest stay folded");
    ui.click(up_at_the_top());
    assert!(!ui.has(up_at_the_top()), "and the rest show the next time");
    ui.click(down_at_the_end());
    assert!(!ui.has(down_at_the_end()), "the seven below show at once, as there are fewer than twenty");
}

/// Over the 4 MB of text the hub sends at once, so its lines are read as they open, rather than with the file.
#[gpui_kit::test]
fn the_lines_of_a_file_too_big_to_read_whole_are_read_as_they_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let lines = (1..=200_000).fold(String::new(), |mut text, n| {
        let _ = writeln!(text, "line {n:>20}");
        text
    });
    let middle = format!("line {:>20}\n", 100_000);
    // Two lines in place of one, so what's below is a line further down on the new side.
    let (mut ui, diff) = changing(cx, &stack, "big.txt", &lines, (&middle, "line one hundred thousand\nand one\n"));
    let (below, further) = (("fold-down", 100_004_usize), ("fold-down", 100_024_usize));
    ui.wait("how many lines it has", |ui| ui.cx.read_entity(&diff.file, |f, _| f.has_read_the_base()));
    assert!(ui.has(below) && ui.has(("fold-up", 1_usize)), "folds that open, above the change and to the end");

    ui.click(below);

    let last = format!("line {:>20}", 100_023);
    ui.wait("the twenty lines below the change", |ui| {
        ui.cx.read_entity(&diff.file, |f, _| f.shows_line(100_024)).is_some_and(|line| line == last)
    });
    assert!(ui.has(further) && !ui.has(below), "the fold starts after them");

    // A change near the top makes the sides again, which keep what was read.
    let worktree = ui.worktree(ui.view().selected.as_ref().unwrap());
    let now = std::fs::read_to_string(worktree.join("big.txt")).unwrap();
    std::fs::write(worktree.join("big.txt"), now.replacen(&format!("line {:>20}\n", 2), "line two\n", 1)).unwrap();
    ui.click("sidebar-tab-changes");
    ui.wait("the sides made again with the change near the top", |ui| {
        ui.cx.read_entity(&diff.file, |f, cx| f.is_ready() && f.file(cx).is_some_and(|f| f.hunks().len() == 2))
    });
    let shown = ui.cx.read_entity(&diff.file, |f, _| f.shows_line(100_024));
    assert_eq!(shown.as_deref(), Some(&*last), "the lines read still show, without being read again");
}

#[gpui_kit::test]
fn what_was_opened_stays_open_when_the_diff_loads_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = changing_a_long_file(cx, &stack);
    ui.click(down_at_the_end());
    let worktree = ui.worktree(ui.view().selected.as_ref().unwrap());
    let lines = std::fs::read_to_string(worktree.join("long.txt")).unwrap();
    std::fs::write(worktree.join("long.txt"), lines.replace("line 2\n", "line two\n")).unwrap();

    ui.click("sidebar-tab-changes");

    ui.wait("the change near the top", |ui| {
        ui.cx.read_entity(&diff.file, |f, cx| f.file(cx).is_some_and(|f| f.hunks().len() == 2))
    });
    assert!(!ui.has(down_at_the_end()), "the lines opened below the change are still open");
}

#[gpui_kit::test]
fn what_was_opened_stays_open_while_another_file_shows_and_folds_once_the_tab_closes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = changing_a_long_file(cx, &stack);
    let worktree = ui.worktree(ui.view().selected.as_ref().unwrap());
    std::fs::write(worktree.join("other.txt"), "other\n").unwrap();
    ui.click("sidebar-tab-changes");
    ui.wait("the other file", |ui| ui.cx.read_entity(&diff.review, |r, _| r.file("other.txt").is_some()));
    ui.click(down_at_the_end());

    ui.open_diff_of(&diff, "other.txt");
    ui.open_diff_of(&diff, "long.txt");
    assert!(!ui.has(down_at_the_end()), "the lines opened below the change are still open");
    ui.press("secondary-w");
    ui.open_diff_of(&diff, "long.txt");

    assert!(ui.has(down_at_the_end()), "and fold again once the tab has closed");
}

#[gpui_kit::test]
fn the_view_chosen_shows_in_every_workspace(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = changing_a_long_file(cx, &stack);
    assert_eq!(shown_as(&ui, &diff), DiffView::Unified);

    ui.click(DiffView::Split.id());
    assert_eq!(shown_as(&ui, &diff), DiffView::Split);
    assert!(ui.has(up_at_the_top()), "side by side folds as one column does");
    ui.click(DiffView::WholeFile.id());
    assert_eq!(shown_as(&ui, &diff), DiffView::WholeFile);
    assert!(!ui.has(up_at_the_top()), "nothing folds in the whole file");

    let second = ui.new_workspace();
    std::fs::write(ui.worktree(&second).join("a.txt"), "one\ntwo\n").unwrap();
    let other = ui.open_changes();
    ui.open_diff_of(&other, "a.txt");
    assert_eq!(shown_as(&ui, &other), DiffView::WholeFile, "a workspace opened since shows it too");
}

#[gpui_kit::test]
fn lines_dragged_across_are_copied_as_the_file_has_them(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = changing_a_long_file(cx, &stack);
    // Just before the start of a line's code, in its numbers, since its button to add a comment is over the start.
    let start = |ui: &mut Ui<'_>, line: u32| {
        ui.frame();
        let id = ElementId::Name(format!("add-comment-new-{line}").into());
        let button = ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap();
        point(button.left() - px(4.), button.center().y)
    };
    let (from, to) = (start(&mut ui, 28), start(&mut ui, 31) + point(px(400.), px(0.)));

    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            window.pointer_move(from, None, cx);
            window.pointer_down(from, MouseButton::Left, cx);
            window.pointer_move(to, Some(MouseButton::Left), cx);
            window.pointer_up(to, MouseButton::Left, cx);
        })
        .unwrap();
    ui.press("secondary-c");

    let copied = "line 28\nline 29\nline 30\nline thirty\nline 31";
    assert_eq!(
        ui.cx.read_entity(&diff.file, |f, _| f.selected_text()).as_deref(),
        Some(copied),
        "the line removed too"
    );
    assert_eq!(ui.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some(copied));
}

#[gpui_kit::test]
fn a_long_line_scrolls_sideways_under_its_numbers(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    for file in ["wide.txt", "also-wide.txt"] {
        std::fs::write(ui.worktree(&workspace).join(file), format!("short\n{}\n", "long ".repeat(200))).unwrap();
    }
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "wide.txt");
    let across = |ui: &Ui<'_>| ui.cx.read_entity(&diff.file, |f, _| f.scrolled_across());
    let button = |ui: &mut Ui<'_>| {
        ui.frame();
        let id = ElementId::Name("add-comment-new-1".into());
        ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
    };
    let numbers = button(&mut ui);

    ui.scroll("diff-lines", ScrollDelta::Pixels(point(px(-120.), px(0.))));

    assert_eq!(across(&ui), px(120.));
    assert_eq!(button(&mut ui), numbers, "the line numbers, and what's over them, stay where they are");
    ui.scroll("diff-lines", ScrollDelta::Pixels(point(px(500.), px(0.))));
    assert_eq!(across(&ui), px(0.), "and it goes back no further than the start");
    ui.scroll("diff-lines", ScrollDelta::Pixels(point(px(-120.), px(0.))));
    ui.open_diff_of(&diff, "also-wide.txt");
    assert_eq!(across(&ui), px(0.), "another file starts at the start of its lines");
}
