//! Opening a terminal, and its ending, as it's closed, as its shell ends, and as the hub restarts under it, with a real
//! shell, and what the terminal tests share.

use desktop::app::Connection;
use desktop::workspace_pane::Showing;
use gpui_kit::{AppContext as _, SharedString, TestAppContext, WeakEntity};
use protocol::model::SessionStatus;
use terminal_view::TerminalView;

use crate::support::{Stack, Ui, hub_environment};

/// A stack whose hubs start each terminal's shell as `/bin/sh`, so it's the same shell for everyone, with the prompt
/// these tests wait for, rather than whichever one whoever runs the tests uses, or the login shell that says nothing,
/// which [`hub_environment`] gives every other test.
pub(super) fn stack() -> Stack {
    let mut environment = hub_environment();
    #[cfg(unix)]
    environment.set("SHELL", "/bin/sh");
    Stack::in_environment(&environment)
}

/// The app on a new workspace, with a terminal opened in it from the `+`, once its shell is ready.
pub(super) fn with_a_terminal<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
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
pub(super) fn type_keys(ui: &mut Ui<'_>, keys: &[&str]) {
    for key in keys {
        ui.press(key);
    }
}

/// Types `text`, a key for each character.
pub(super) fn type_text(ui: &mut Ui<'_>, text: &str) {
    let typed = Vec::from_iter(text.chars().map(|key| if key == ' ' { "space".to_owned() } else { key.to_string() }));
    type_keys(ui, &Vec::from_iter(typed.iter().map(String::as_str)));
}

/// Types `line`, and Return.
pub(super) fn type_line(ui: &mut Ui<'_>, line: &str) {
    type_text(ui, line);
    ui.press("enter");
}

/// Waits for the terminal in front to show a row that says `text` and nothing else, as the shell prints it, which the
/// row with the command typed for it never is.
pub(super) fn wait_for_a_row(ui: &mut Ui<'_>, what: &str, text: &str) {
    ui.wait(what, |ui| rows(ui).iter().any(|row| row.trim_end() == text));
}

pub(super) fn rows(ui: &Ui<'_>) -> Vec<String> {
    let Some(terminal) = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.terminal()) else { return Vec::new() };
    ui.cx.read_entity(&terminal, |t, _| t.rows())
}

pub(super) fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

/// The view of the terminal in front, which shouldn't outlive the terminal, since the terminal it holds keeps
/// client-core's copy of all it shows.
fn view(ui: &Ui<'_>) -> WeakEntity<TerminalView> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.terminal()).unwrap().downgrade()
}

fn type_echo_hi(ui: &mut Ui<'_>) {
    type_line(ui, "echo hi");
    wait_for_a_row(ui, "what the shell printed", "hi");
}

/// Checks that the composer showing has the keys, by typing into it, whatever it held before.
fn the_composer_has_the_keys(ui: &mut Ui<'_>, case: &str) {
    let before = ui.composer_text();
    ui.input("next");
    assert_eq!(ui.composer_text(), format!("{before}next"), "{case}: the chat's composer has the keys");
}

/// However a terminal ends, its tab goes with it, and the chat comes back with the keys. Each way is taken in turn,
/// in one window, each with a terminal of its own: its shell ending, as `exit` ends it, so the tab doesn't stay for a
/// terminal that's gone, then closing one opened with ⇧⌘T, as ⌘T opens a chat, which ends its shell, then one that
/// ends in a workspace behind, whose pane isn't drawn, so the keys stay with the workspace that is.
#[gpui_kit::test]
fn a_terminal_that_ends_takes_its_tab_with_it_and_leaves_the_keys_to_the_chat(cx: &mut TestAppContext) {
    let stack = stack();
    let mut ui = with_a_terminal(cx, &stack);

    its_shell_ending(&mut ui, &stack);
    closing_one_opened_with_its_key(&mut ui, &stack);
    ending_in_a_workspace_behind(&mut ui, &stack);
}

/// A reply that came while the terminal covered its chat is read once the chat shows in the terminal's place, though
/// nothing on the chat's record changes then. Only a chat in the window in front is read, so the window is brought
/// there first.
fn its_shell_ending(ui: &mut Ui<'_>, stack: &Stack) {
    let view = view(ui);
    ui.bring_to_front();
    let chat = ui.active_chat();
    stack.client().session_prompt(&chat, "say:while the terminal was in front", None).unwrap();
    let finished_unread = |ui: &mut Ui<'_>| {
        let session = ui.view().records().session(&chat).cloned();
        session.is_some_and(|s| s.unread() && s.status == SessionStatus::Idle)
    };
    ui.wait("exit: the reply, finished and unread behind the terminal", finished_unread);

    type_line(ui, "exit");

    ui.wait("exit: the chat in front, with no terminal", |ui| showing(ui) == Showing::Chat && ui.pane().is_some());
    crate::support::wait_until("exit: the terminal's record to go", || {
        stack.client().records_list().unwrap().terminals.is_empty()
    });
    ui.wait("exit: the reply to be read", |_| !stack.client().session_get(&chat).unwrap().unread());
    assert!(!ui.has(terminal_view::SHOWN_ID), "exit: the terminal is gone");
    the_composer_has_the_keys(ui, "exit");
    ui.wait("exit: the view to be let go", |_| view.upgrade().is_none());
}

fn closing_one_opened_with_its_key(ui: &mut Ui<'_>, stack: &Stack) {
    ui.press("secondary-shift-t");
    wait_for_the_prompt(ui);
    type_echo_hi(ui);
    assert_eq!(stack.client().records_list().unwrap().terminals.len(), 1);
    let file = crate::menus::items(ui, "File");
    assert!(file.iter().any(|item| item == "Close Terminal"), "it closes the terminal, not the chat behind it");
    let view = view(ui);

    ui.press(CLOSE);

    ui.wait("closing: the chat in front", |ui| showing(ui) == Showing::Chat);
    crate::support::wait_until("closing: the terminal to close", || {
        stack.client().records_list().unwrap().terminals.is_empty()
    });
    the_composer_has_the_keys(ui, "closing");
    ui.wait("closing: the view to be let go", |_| view.upgrade().is_none());
}

fn ending_in_a_workspace_behind(ui: &mut Ui<'_>, stack: &Stack) {
    let behind = ui.view().selected.clone().unwrap();
    ui.pick_item("new-tab", "New terminal");
    ui.wait("behind: the terminal to show", |ui| ui.has(terminal_view::SHOWN_ID));
    ui.new_workspace();
    let pane = ui.view().panes.get(&behind).cloned().unwrap();
    let terminal = stack.client().records_list().unwrap().terminals[0].id.clone();

    stack.client().terminal_close(&terminal).unwrap();

    ui.wait("behind: the chat in front behind", |ui| ui.cx.read_entity(&pane, |p, _| p.showing()) == Showing::Chat);
    the_composer_has_the_keys(ui, "behind: the composer on screen");
}

/// The worker carries on with its shell through the hub's crash, so the terminal is still there, and the view takes it
/// up on the new hub. Then, as the hub restarts for an update, the workspace stays on screen while the app reconnects,
/// so the chat's tab brought to the front then gives its composer the keys, rather than leave them with the terminal it
/// covers.
#[gpui_kit::test]
fn a_terminal_carries_on_as_the_hub_restarts_and_a_tab_brought_in_front_of_it_takes_the_keys(cx: &mut TestAppContext) {
    let stack = stack();
    let mut ui = with_a_terminal(cx, &stack);
    let view = view(&ui);

    let pid = stack.crash_hub();

    ui.wait_for_new_hub(&stack, pid);
    ui.wait("the terminal to be open on the new hub", |_| stack.client().records_list().unwrap().terminals.len() == 1);
    assert_eq!(showing(&ui), Showing::Terminal);
    assert!(self::view(&ui) == view, "the same view, taken up again");
    // What's typed before the view's copy follows the terminal on the new hub is dropped, as over a lost connection.
    ui.wait("the view to follow the terminal on the new hub", |ui| {
        ui.cx.read_entity(&view.upgrade().unwrap(), |view, _| view.open_terminal().connected())
    });
    type_echo_hi(&mut ui);

    let chat = ui.active_chat();
    let pid = stack.client().hub_status().unwrap().pid;
    // Clicked before anything else runs, so the app is surely still connecting.
    ui.cx.update(|cx| ui.view.update(cx, |view, cx| view.restart_hub(pid, cx)));
    assert!(matches!(ui.view().connection(), Connection::Connecting));
    ui.click(SharedString::from(format!("chat-{chat}")));
    ui.input("kept");

    assert_eq!(showing(&ui), Showing::Chat);
    assert_eq!(ui.composer_text(), "kept");
    ui.wait_for_new_hub(&stack, pid);
}
