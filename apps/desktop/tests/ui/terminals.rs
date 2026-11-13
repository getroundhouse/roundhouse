//! A workspace's terminals in the app, with a real shell: one opened from the `+` after the tabs takes the keys, and
//! its tab goes once it's closed or its shell ends, with the chat back in front.

#![cfg(test)]

use desktop::app::Connection;
use desktop::workspace_pane::Showing;
use gpui_kit::{AppContext as _, ClipboardItem, SharedString, TestAppContext, WeakEntity};
use terminal_view::TerminalView;

use crate::support::{Stack, Ui};

/// The app on a new workspace, with a terminal opened in it from the `+`, once its shell is ready.
fn with_a_terminal<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    ui.new_workspace();
    ui.pick_item("new-tab", "New terminal");
    wait_for_the_prompt(&mut ui);
    ui
}

/// Waits for the shell's prompt, which `/bin/sh` ends with `$`, or `#` as root. What's typed before it is echoed as it
/// arrives, so what it prints would follow the prompt on the same row, rather than have a row of its own, where a
/// shell such as dash, which shows nothing typed again, starts slowly, as its login does on Ubuntu.
fn wait_for_the_prompt(ui: &mut Ui<'_>) {
    ui.wait("the shell's prompt", |ui| {
        ui.has(terminal_view::SHOWN_ID) && rows(ui).iter().any(|row| row.trim_end().ends_with(['$', '#']))
    });
}

/// What closes the terminal in front: Cmd-W on a Mac, and elsewhere Ctrl-Shift-W, since Ctrl-W is its program's there.
const CLOSE: &str = if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-shift-w" };

/// Each key, pressed as a hand presses it, into what has focus.
fn type_keys(ui: &mut Ui<'_>, keys: &[&str]) {
    for key in keys {
        ui.press(key);
    }
}

fn rows(ui: &Ui<'_>) -> Vec<String> {
    let Some(terminal) = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.terminal()) else { return Vec::new() };
    ui.cx.read_entity(&terminal, |t, _| t.rows())
}

fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

/// The view of the terminal in front, which shouldn't outlive the terminal, since the terminal it holds keeps
/// client-core's copy of all it shows.
fn view(ui: &Ui<'_>) -> WeakEntity<TerminalView> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.terminal()).unwrap().downgrade()
}

fn type_echo_hi(ui: &mut Ui<'_>) {
    type_keys(ui, &["e", "c", "h", "o", "space", "h", "i", "enter"]);
    ui.wait("what the shell printed", |ui| rows(ui).iter().any(|row| row.trim_end() == "hi"));
}

/// What's typed reaches the shell, which a new terminal has the keys for, and what it prints shows.
#[gpui_kit::test]
fn a_terminal_opened_from_the_plus_takes_the_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);

    type_echo_hi(&mut ui);

    assert_eq!(showing(&ui), Showing::Terminal);
}

/// What's copied goes to the shell as if it were typed, with the terminal's paste key.
#[gpui_kit::test]
fn what_is_pasted_reaches_the_shell(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    // Quoted so the shell prints it whole, and its echo of what was pasted doesn't count.
    ui.cx.write_to_clipboard(ClipboardItem::new_string("echo pas''ted".to_owned()));

    ui.press(if cfg!(target_os = "macos") { "cmd-v" } else { "ctrl-shift-v" });
    ui.press("enter");

    ui.wait("what the shell printed", |ui| rows(ui).iter().any(|row| row.trim_end() == "pasted"));
}

/// As the paste key does.
#[gpui_kit::test]
fn the_edit_menus_paste_pastes_in_a_terminal(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    ui.cx.write_to_clipboard(ClipboardItem::new_string("echo pas''ted".to_owned()));

    crate::menus::choose(&mut ui, "Edit", "Paste");
    ui.press("enter");

    ui.wait("what the shell printed", |ui| rows(ui).iter().any(|row| row.trim_end() == "pasted"));
}

/// As ⌘T opens a chat.
#[gpui_kit::test]
fn shift_command_t_opens_a_terminal_that_takes_the_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();

    ui.press("secondary-shift-t");

    wait_for_the_prompt(&mut ui);
    type_echo_hi(&mut ui);
}

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

/// The worker carries on with its shell, so the terminal is still there, and the view takes it up on the new hub.
#[gpui_kit::test]
fn a_terminal_carries_on_after_the_hub_restarts(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    let view = view(&ui);

    let pid = stack.crash_hub();

    ui.wait_for_new_hub(&stack, pid);
    ui.wait("the terminal to be open on the new hub", |_| stack.client().records_list().unwrap().terminals.len() == 1);
    assert_eq!(showing(&ui), Showing::Terminal);
    assert!(crate::terminals::view(&ui) == view, "the same view, taken up again");
    // What's typed before the view's copy follows the terminal on the new hub is dropped, as over a lost connection.
    ui.wait("the view to follow the terminal on the new hub", |ui| {
        ui.cx.read_entity(&view.upgrade().unwrap(), |view, _| view.open_terminal().connected())
    });
    type_echo_hi(&mut ui);
}

/// The workspace stays on screen while the app reconnects, as through the hub's restart for an update, so the chat's
/// tab brought to the front then gives its composer the keys, rather than leave them with the terminal it covers.
#[gpui_kit::test]
fn a_tab_brought_to_the_front_as_the_hub_restarts_takes_the_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    let chat = ui.active_chat();
    let pid = stack.client().hub_status().unwrap().pid;

    // Clicked before anything else runs, so the app is surely still connecting.
    ui.cx.update(|cx| ui.view.update(cx, |view, cx| view.restart_hub(pid, cx)));
    assert!(matches!(ui.view().connection, Connection::Connecting));
    ui.click(SharedString::from(format!("chat-{chat}")));
    ui.input("kept");

    assert_eq!(showing(&ui), Showing::Chat);
    assert_eq!(ui.composer_text(), "kept");
    ui.wait_for_new_hub(&stack, pid);
}

#[gpui_kit::test]
fn closing_a_terminal_ends_it_and_brings_back_the_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    assert_eq!(stack.client().records_list().unwrap().terminals.len(), 1);
    let file = crate::menus::items(&ui, "File");
    assert!(file.iter().any(|item| item == "Close Terminal"), "it closes the terminal, not the chat behind it");

    let view = view(&ui);

    ui.press(CLOSE);

    ui.wait("the chat in front", |ui| showing(ui) == Showing::Chat);
    crate::support::wait_until("the terminal to close", || stack.client().records_list().unwrap().terminals.is_empty());
    ui.input("next");
    assert_eq!(ui.composer_text(), "next", "the chat's composer has the keys again");
    ui.wait("the view to be let go", |_| view.upgrade().is_none());
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

/// Its pane isn't drawn, so the keys stay with the workspace that is.
#[gpui_kit::test]
fn a_terminal_that_ends_in_a_workspace_behind_leaves_the_keys_where_they_are(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let behind = ui.new_workspace();
    ui.pick_item("new-tab", "New terminal");
    ui.wait("the terminal to show", |ui| ui.has(terminal_view::SHOWN_ID));
    ui.new_workspace();
    let pane = ui.view().panes.get(&behind).cloned().unwrap();
    let terminal = stack.client().records_list().unwrap().terminals[0].id.clone();

    stack.client().terminal_close(&terminal).unwrap();

    ui.wait("the chat in front behind", |ui| ui.cx.read_entity(&pane, |p, _| p.showing()) == Showing::Chat);
    ui.input("next");
    assert_eq!(ui.composer_text(), "next", "the composer on screen kept the keys");
}

/// As `exit` ends it, so the tab doesn't stay for a terminal that's gone.
#[gpui_kit::test]
fn a_terminal_whose_shell_ends_takes_its_tab_with_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_terminal(cx, &stack);
    let view = view(&ui);

    type_keys(&mut ui, &["e", "x", "i", "t", "enter"]);

    ui.wait("the chat in front, with no terminal", |ui| showing(ui) == Showing::Chat && ui.pane().is_some());
    crate::support::wait_until("the terminal's record to go", || {
        stack.client().records_list().unwrap().terminals.is_empty()
    });
    assert!(!ui.has(terminal_view::SHOWN_ID));
    ui.input("next");
    assert_eq!(ui.composer_text(), "next", "the chat's composer has the keys again");
    ui.wait("the view to be let go", |_| view.upgrade().is_none());
}
