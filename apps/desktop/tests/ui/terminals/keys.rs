//! A terminal's keys: each is its program's, as in terminal apps, but for the app's own, and on a Mac the keys Ghostty
//! gives a meaning edit the shell's line as they do there.

use desktop::workspace_pane::Showing;
use gpui_kit::component::input::{Copy, Paste, SelectAll};
use gpui_kit::{Action, TestAppContext};
use protocol::model::WorkspaceStatus;

use super::{rows, showing, type_echo_hi, type_keys, with_a_terminal};
use crate::support::{Stack, Ui};

/// Tab and Shift-Tab are the program's, as in terminal apps, rather than moving the keys to what's next on screen, as
/// the kit has them do everywhere else. `cat` gives back each line it reads, so a tab shows where it took the cursor.
#[gpui_kit::test]
fn tab_and_shift_tab_reach_the_program(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    let tabbed = |ui: &Ui<'_>, text: &str| rows(ui).iter().filter(|row| row.trim_end() == text).count() == 2;

    type_keys(&mut ui, &["c", "a", "t", "enter", "tab", "x", "enter"]);
    ui.wait("the tabbed line, as typed and as given back", |ui| tabbed(ui, "        x"));
    type_keys(&mut ui, &["shift-tab", "enter", "y", "enter"]);

    ui.wait("the line after Shift-Tab, as typed and as given back", |ui| tabbed(ui, "y"));
}

/// As it moves between chats, coming to the terminals after them, and wrapping round.
#[gpui_kit::test]
fn ctrl_tab_moves_between_the_chat_and_a_terminal(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);

    ui.press("ctrl-tab");
    ui.wait("the chat in front", |ui| showing(ui) == Showing::Chat);
    ui.input("next");
    assert_eq!(ui.composer_text(), "next", "the chat's composer has the keys");
    ui.press("ctrl-tab");

    ui.wait("the terminal in front", |ui| showing(ui) == Showing::Terminal);
    type_echo_hi(&mut ui);
}

/// A terminal's keys are its program's, as in terminal apps, but the app's on Command on a Mac and on Ctrl and Shift
/// elsewhere, Ctrl-Tab and Ctrl-Shift-Tab on both, and the terminal's own. So every key bound where a terminal has the
/// keys is one of those, whatever shortcuts the app has, and off the Mac, the app's others, such as Ctrl-W, Ctrl-, and
/// F10, aren't bound there.
#[gpui_kit::test]
fn a_terminal_leaves_its_program_every_key_but_the_apps_own(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    let close = if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-shift-w" };

    let bound = crate::menus::every_key_bound_where_focus_is(&mut ui);
    assert!(bound.iter().any(|(_, action)| action.partial_eq(&desktop::CloseChat)), "it finds the app's keys too");

    for (keys, action) in &bound {
        let held = keys.modifiers;
        let apps = if cfg!(target_os = "macos") { held.platform } else { held.control && held.shift };
        let between_tabs = held.control && keys.key == "tab";
        let terminals_own = action.name().starts_with("terminal::")
            || [&Copy as &dyn Action, &Paste, &SelectAll].into_iter().any(|own| action.partial_eq(own));
        assert!(apps || between_tabs || terminals_own, "{keys:?} is bound to {} in a terminal", action.name());
    }
    for (key, action) in [(close, &desktop::CloseChat as &dyn Action), ("ctrl-tab", &desktop::NextChat)] {
        let bound = crate::menus::bound_where_focus_is(&mut ui, key);
        assert!(bound.is_some_and(|bound| bound.partial_eq(action)), "{key} is the app's");
    }
}

/// Stop Agent acts on the chat showing, so a terminal in front of it doesn't take it, and its key, Ctrl-. off the Mac,
/// goes to the program.
#[gpui_kit::test]
fn stop_agent_is_left_to_the_chat_showing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    let can_stop = |ui: &mut Ui<'_>| {
        ui.frame();
        ui.cx.update(|cx| cx.is_action_available(&desktop::StopAgent))
    };
    assert!(!can_stop(&mut ui));

    ui.press("ctrl-tab");

    ui.wait("the chat in front", |ui| showing(ui) == Showing::Chat);
    assert!(can_stop(&mut ui));
}

/// Archive's key is a terminal's Select All off the Mac, so there the File menu archives the workspace a terminal is in.
#[gpui_kit::test]
fn the_file_menu_archives_the_workspace_a_terminal_is_in(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);

    crate::menus::choose(&mut ui, "File", "Archive Workspace");

    ui.wait("it to be archived", |ui| {
        ui.view().records().workspaces().iter().all(|w| w.status == WorkspaceStatus::Archived)
    });
}

/// Off the Mac, where the app's shortcuts are on Ctrl, as Ctrl-W closes a chat, in a terminal it deletes the word
/// before it, as a shell has it. On a Mac, where Ctrl-W was never the app's, the same holds.
#[gpui_kit::test]
fn ctrl_w_in_a_terminal_reaches_its_shell(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);

    type_keys(&mut ui, &["e", "c", "h", "o", "space", "h", "i", "space", "t", "h", "e", "r", "e", "ctrl-w", "enter"]);

    ui.wait("what the shell printed", |ui| rows(ui).iter().any(|row| row.trim_end() == "hi"));
    assert_eq!(showing(&ui), Showing::Terminal, "the terminal is still open");
}

/// On a Mac, the keys Ghostty gives a meaning there edit the shell's line as they do in Ghostty: Command-Backspace
/// deletes back to its start, as `/bin/sh`, which is bash, reads it, Command-Left and Command-Right go to its start and
/// its end, and Option-Left and Option-Right go back a word and on one. Each puts what's typed after it somewhere only
/// it would.
#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn ghosttys_mac_keys_edit_the_shells_line(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);

    type_keys(&mut ui, &["x", "x", "cmd-backspace", "o", "n", "e", "space", "t", "w", "o"]);
    type_keys(&mut ui, &["cmd-left", "e", "c", "h", "o", "space", "alt-right", "s"]);
    type_keys(&mut ui, &["cmd-right", "space", "x", "alt-left", "y", "enter"]);

    ui.wait("what the shell printed", |ui| rows(ui).iter().any(|row| row.trim_end() == "ones two yx"));
}
