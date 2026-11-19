//! Dictated comments follow the existing draft and save paths, and a hidden comment releases its recording.

use gpui_kit::{AppContext as _, Focusable as _, TestAppContext};

use super::support::{hear, hearing, hold_option_space, let_go, live, ready};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn a_comment_can_be_dictated_cancelled_and_saved_with_its_last_words(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let engine = ready(&mut ui);
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Please");
    hold_option_space(&mut ui);
    let audio = engine.saying("fix the test. / in hub");
    hear(&mut ui, &audio);
    hearing(&mut ui, "in");
    assert!(!ui.has("save-comment") && !ui.has("cancel-comment"));

    ui.click(dictation_ui::CANCEL);
    let_go(&mut ui);
    assert_eq!(ui.cx.read_entity(&diff.file, |f, cx| f.writing(cx).unwrap().text), "Please");
    assert!(ui.has("save-comment"));
    hold_option_space(&mut ui);
    hear(&mut ui, &audio);
    hearing(&mut ui, "in");
    ui.press("enter");

    ui.wait("a comment with its final sentence", |ui| {
        ui.comments(&diff).iter().any(|c| c.text == "Please Fix the test. In hub.")
    });
    assert!(live(&mut ui).is_none());
}

#[gpui_kit::test]
fn hiding_the_diff_ends_its_recording_and_keeps_only_final_text(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let engine = ready(&mut ui);
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);
    hold_option_space(&mut ui);
    let audio = engine.saying("fix the test. / in hub");
    hear(&mut ui, &audio);
    hearing(&mut ui, "in");

    ui.press("secondary-w");
    let_go(&mut ui);

    assert!(!ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().unwrap().dictation.read(cx).is_dictating()));
    assert_eq!(ui.cx.read_entity(&diff.file, |f, cx| f.writing(cx).unwrap().text), "Fix the test.");
    ui.open_diff_of(&diff, "a.txt");
    assert!(live(&mut ui).is_none());
    assert!(ui.has("save-comment"));
}

#[gpui_kit::test]
fn a_refresh_that_removes_the_diff_ends_dictation_in_its_retained_comment(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let engine = ready(&mut ui);
    let path = ui.worktree(&workspace).join("a.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);
    hold_option_space(&mut ui);
    let audio = engine.saying("fix the test. / in hub");
    hear(&mut ui, &audio);
    hearing(&mut ui, "in");

    std::fs::write(&path, "one\n").unwrap();
    ui.cx.update_entity(&diff.review, desktop::review::Review::refresh);
    ui.wait("the comment to leave the refreshed diff", |ui| {
        ui.cx.read_entity(&diff.file, |f, cx| f.file(cx).is_none()) && !ui.has(dictation_ui::STOP)
    });

    assert!(!ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().unwrap().dictation.read(cx).is_dictating()));
    let_go(&mut ui);
    assert_eq!(ui.cx.read_entity(&diff.file, |f, cx| f.writing(cx).unwrap().text), "Fix the test.");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    ui.cx.update_entity(&diff.review, desktop::review::Review::refresh);
    ui.wait("the retained comment to return", |ui| {
        ui.has("save-comment") && ui.cx.read_entity(&diff.file, |f, _| f.is_ready())
    });
    assert!(live(&mut ui).is_none());
    assert!(
        ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().unwrap().dictation.read(cx).can_dictate(cx)),
        "the returned comment is enabled"
    );
    ui.click("input-state");
    assert!(
        ui.cx
            .update_window(ui.handle, |_, window, cx| {
                diff.file.read(cx).draft.as_ref().unwrap().input.focus_handle(cx).is_focused(window)
            })
            .unwrap()
    );
    hold_option_space(&mut ui);
    assert!(
        ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().unwrap().dictation.read(cx).is_dictating()),
        "the first press after the comment returns starts recording"
    );
    hear(&mut ui, &audio);
    hearing(&mut ui, "in");
    ui.press("alt-escape");
}
