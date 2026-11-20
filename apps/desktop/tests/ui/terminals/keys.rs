//! A terminal's keys: each is its program's, as in terminal apps, but for the app's own, and on a Mac the keys Ghostty
//! gives a meaning edit the shell's line as they do there.

use desktop::workspace_pane::Showing;
use gpui_kit::{Action, ClipboardItem, TestAppContext};
use protocol::model::WorkspaceStatus;

use super::selecting::with_a_made_up_terminal;
use super::{rows, showing, stack, type_keys, type_line, type_text, wait_for_a_row, with_a_terminal};
use crate::support::Ui;

/// A way the keys reach a terminal's shell, each printing what only it prints.
#[derive(Clone, Copy, Debug)]
enum Reaching {
    /// What's typed in a terminal opened from the `+`, which has the keys.
    Typed,
    /// Off the Mac, where the app's shortcuts are on Ctrl, as Ctrl-W closes a chat, in a terminal it deletes the word
    /// before it, as a shell has it. On a Mac, where Ctrl-W was never the app's, the same holds.
    CtrlW,
    /// Ctrl-C interrupts the program, as in terminal apps, though off the Mac it's Copy everywhere else, so `cat` ends
    /// and the shell runs what's typed after.
    CtrlC,
    /// On a Mac, the keys Ghostty gives a meaning there edit the shell's line as they do in Ghostty: Command-Backspace
    /// deletes back to its start, as `/bin/sh`, which is bash, reads it, Command-Left and Command-Right go to its start
    /// and its end, and Option-Left and Option-Right go back a word and on one. Each puts what's typed after it
    /// somewhere only it would.
    GhosttysMacKeys,
    /// What's copied goes to the shell as if it were typed, with the terminal's paste key.
    ThePasteKey,
    /// As the paste key does.
    TheEditMenusPaste,
    /// Ctrl-Tab moves between chats, coming to the terminals after them, and wrapping round, so it goes to the chat,
    /// whose composer then has the keys, and back.
    BackFromTheChat,
}

impl Reaching {
    fn keys(self, ui: &mut Ui<'_>) {
        match self {
            Reaching::Typed => type_line(ui, "echo typed"),
            Reaching::CtrlW => {
                type_text(ui, "echo worded there");
                type_keys(ui, &["ctrl-w", "enter"]);
            }
            Reaching::CtrlC => {
                type_line(ui, "cat");
                ui.press("ctrl-c");
                type_line(ui, "echo interrupted");
            }
            Reaching::GhosttysMacKeys => {
                type_keys(ui, &["x", "x", "cmd-backspace", "o", "n", "e", "space", "t", "w", "o"]);
                type_keys(ui, &["cmd-left", "e", "c", "h", "o", "space", "alt-right", "s"]);
                type_keys(ui, &["cmd-right", "space", "x", "alt-left", "y", "enter"]);
            }
            // Quoted so the shell prints it whole, and its echo of what was pasted doesn't count.
            Reaching::ThePasteKey => {
                ui.cx.write_to_clipboard(ClipboardItem::new_string("echo pas''ted".to_owned()));
                ui.press(if cfg!(target_os = "macos") { "cmd-v" } else { "ctrl-shift-v" });
                ui.press("enter");
            }
            Reaching::TheEditMenusPaste => {
                ui.cx.write_to_clipboard(ClipboardItem::new_string("echo pas''ted from the menu".to_owned()));
                crate::menus::choose(ui, "Edit", "Paste");
                ui.press("enter");
            }
            Reaching::BackFromTheChat => {
                ui.press("ctrl-tab");
                ui.wait("BackFromTheChat: the chat in front", |ui| showing(ui) == Showing::Chat);
                ui.input("next");
                assert_eq!(ui.composer_text(), "next", "BackFromTheChat: the chat's composer has the keys");
                ui.press("ctrl-tab");
                ui.wait("BackFromTheChat: the terminal in front", |ui| showing(ui) == Showing::Terminal);
                type_line(ui, "echo back''again");
            }
        }
    }

    /// The row the shell prints for it, and nothing else does.
    fn prints(self) -> &'static str {
        match self {
            Reaching::Typed => "typed",
            Reaching::CtrlW => "worded",
            Reaching::CtrlC => "interrupted",
            Reaching::GhosttysMacKeys => "ones two yx",
            Reaching::ThePasteKey => "pasted",
            Reaching::TheEditMenusPaste => "pasted from the menu",
            Reaching::BackFromTheChat => "backagain",
        }
    }
}

/// Each way in turn, in one terminal, then Tab and Shift-Tab, then archiving from the File menu, which ends it. Each
/// waits for the row only it prints, a line of a few words, so the terminal has little to draw again.
#[gpui_kit::test]
fn whats_typed_and_pasted_in_a_terminal_reaches_its_shell(cx: &mut TestAppContext) {
    let stack = stack();
    let mut ui = with_a_terminal(cx, &stack);
    let mut ways = vec![Reaching::Typed, Reaching::CtrlW, Reaching::CtrlC];
    if cfg!(target_os = "macos") {
        ways.push(Reaching::GhosttysMacKeys);
    }
    ways.extend([Reaching::ThePasteKey, Reaching::TheEditMenusPaste, Reaching::BackFromTheChat]);

    for way in ways {
        way.keys(&mut ui);

        wait_for_a_row(&mut ui, &format!("{way:?}: what the shell printed"), way.prints());
        assert_eq!(showing(&ui), Showing::Terminal, "{way:?}: the terminal is still in front");
    }
    tab_and_shift_tab_reach_the_program(&mut ui);
    the_file_menu_archives_the_workspace_a_terminal_is_in(&mut ui);
}

/// Tab and Shift-Tab are the program's, as in terminal apps, rather than moving the keys to what's next on screen, as
/// the kit has them do everywhere else. `cat` gives back each line it reads, so a tab shows where it took the cursor.
fn tab_and_shift_tab_reach_the_program(ui: &mut Ui<'_>) {
    let tabbed = |ui: &Ui<'_>, text: &str| rows(ui).iter().filter(|row| row.trim_end() == text).count() == 2;

    type_keys(ui, &["c", "a", "t", "enter", "tab", "x", "enter"]);
    ui.wait("Tab: the tabbed line, as typed and as given back", |ui| tabbed(ui, "        x"));
    type_keys(ui, &["shift-tab", "enter", "y", "enter"]);

    ui.wait("Shift-Tab: the line after it, as typed and as given back", |ui| tabbed(ui, "y"));
}

/// Archive's key is a terminal's Select All off the Mac, so there the File menu archives the workspace a terminal is in.
fn the_file_menu_archives_the_workspace_a_terminal_is_in(ui: &mut Ui<'_>) {
    crate::menus::choose(ui, "File", "Archive Workspace");

    ui.wait("it to be archived", |ui| {
        ui.view().records().workspaces().iter().all(|w| w.status == WorkspaceStatus::Archived)
    });
}

/// A terminal's keys are its program's, as in terminal apps, but the app's on Command on a Mac and on Ctrl and Shift
/// elsewhere, Ctrl-Tab and Ctrl-Shift-Tab on both, and the terminal's own. So every key bound where a terminal has the
/// keys is one of those, whatever shortcuts the app has, and off the Mac, the app's others, such as Ctrl-W, Ctrl-, and
/// F10, aren't bound there. The terminal's Copy, Paste and Select All count only on the app's keys, since off the Mac
/// the kit binds Ctrl-C to Copy. Which keys are bound is the app's own, so a fake hub's terminal does.
#[gpui_kit::test]
fn a_terminal_leaves_its_program_every_key_but_the_apps_own(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_made_up_terminal(cx);
    let close = if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-shift-w" };

    let bound = crate::menus::every_key_bound_where_focus_is(&mut ui);
    assert!(bound.iter().any(|(_, action)| action.partial_eq(&desktop::CloseChat)), "it finds the app's keys too");

    for (keys, action) in &bound {
        let held = keys.modifiers;
        // Shift with a bracket comes as the brace it types, with Shift taken off.
        let shifted = held.shift || ["{", "}"].contains(&keys.key.as_str());
        let apps = if cfg!(target_os = "macos") { held.platform } else { held.control && shifted };
        let between_tabs = held.control && keys.key == "tab";
        let terminals_own = action.name().starts_with("terminal::");
        assert!(apps || between_tabs || terminals_own, "{keys:?} is bound to {} in a terminal", action.name());
    }
    for (key, action) in [(close, &desktop::CloseChat as &dyn Action), ("ctrl-tab", &desktop::NextChat)] {
        let bound = crate::menus::bound_where_focus_is(&mut ui, key);
        assert!(bound.is_some_and(|bound| bound.partial_eq(action)), "{key} is the app's");
    }
}

/// Stop Agent acts on the chat showing, so a terminal in front of it doesn't take it, and its key, Ctrl-. off the Mac,
/// goes to the program. Which chat shows is the app's own, so a fake hub's terminal does.
#[gpui_kit::test]
fn stop_agent_is_left_to_the_chat_showing(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_made_up_terminal(cx);
    let can_stop = |ui: &mut Ui<'_>| {
        ui.frame();
        ui.cx.update(|cx| cx.is_action_available(&desktop::StopAgent))
    };
    assert!(!can_stop(&mut ui));

    ui.press("ctrl-tab");

    ui.wait("the chat in front", |ui| showing(ui) == Showing::Chat);
    assert!(can_stop(&mut ui));
}
