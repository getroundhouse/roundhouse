//! The app's shortcuts. Each that a Mac has on Cmd is bound with `secondary`, which is Cmd on a Mac and Ctrl elsewhere,
//! since off the Mac `cmd` is the Windows or Super key, which the system keeps for itself.
//!
//! A shortcut bound with no context takes its key wherever focus is, even from a view that binds the same key deeper
//! in, since GPUI ranks a binding without a context with the deepest. Where both act, the app's wins, as it's bound
//! after the kit's, and GPUI goes on to the view's only where nothing takes the app's. So the kit's text boxes keep
//! their own Ctrl keys only because none of their editing keys is one of these that has no context. The few they
//! share, Ctrl with F, [, ] and ., which a text box binds to search, outdent, indent and its code actions, the app
//! takes from them on a Mac too, with Cmd, and Cmd+Option with Up and Down, Ctrl+Alt on Windows, with which a text box
//! adds a cursor above or below. A view that needs one of these for itself, as a terminal does Ctrl+W, has to be left
//! out of where it's bound.
//!
//! In a terminal, every key is its program's but the app's on Command on a Mac and on Ctrl and Shift elsewhere, and
//! Ctrl+Tab and Ctrl+Shift+Tab on both, which terminal apps keep for themselves, as Ghostty does. So off the Mac, the
//! app's other shortcuts, on Ctrl with a letter, `[`, `]`, `,`, `.` or an arrow, and on F3 and F10, are left out of a
//! terminal, as Ctrl+W deletes a word in a shell, Ctrl+[ is Escape, and F10 is a key full-screen programs take. In a
//! terminal there, Close, New Workspace and Quit take Shift as well, as terminal apps have them, and New Chat,
//! Settings, hiding the sidebar, Back, Forward and the workspace above and below are in the menus, which the button in
//! the corner opens. Find and Stop Agent are a chat's, so they act only while one shows, as `app::shortcuts` has
//! them, which on a Mac too keeps their keys from acting on a chat out of sight.
//!
//! Archive is left out of a terminal too, since Ctrl+Shift+A is a terminal's Select All there, as
//! `terminal_view::bind_keys` has it, so in a terminal Archive is the File menu's.
//!
//! Page Up and Page Down scroll a chat from its composer, and a text box in a question in its place, where a text box
//! would move its cursor a screen with them, so they're bound there after the kit's, which they then win over.

use std::rc::Rc;

use gpui_kit::base::actions::{SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::component::input::{
    Copy, MoveDown, MovePageDown, MovePageUp, MoveToEnd, MoveToStart, MoveUp, SelectAll, SelectToNextWordEnd,
    SelectToPreviousWordStart,
};
use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate};

use crate::workspace_pane::CHAT_KEY_CONTEXT;
use crate::{
    ArchiveWorkspace, BackToComposer, ChatPageDown, ChatPageUp, CloseChat, CloseSettings, ConversationMode,
    CreatePullRequest, FindInChat, FindNext, FindPrevious, FindSetting, FlipImage, GoBack, GoForward, Hide, HideOthers,
    MarkWorkspace, Minimize, NewChat, NewTerminal, NewWorkspace, NextChat, NextWorkspace, OpenSettings, PreviousChat,
    PreviousWorkspace, Quit, SelectStepBack, SelectStepOn, StepBack, StepOn, StopAgent, ToggleDiff, ToggleMenus,
    TogglePlanMode, ToggleSidebar, composing, file_diff, image_diff, settings,
};

/// A Mac's own shortcuts. Only a Mac hides an app, and elsewhere the system minimizes a window itself.
fn set_a_macs_own(cx: &mut App) {
    if cx.platform_info().is_mac() {
        cx.bind_keys([
            key("secondary-h", Hide, None, cx),
            key("alt-secondary-h", HideOthers, None, cx),
            key("secondary-m", Minimize, None, cx),
        ]);
    }
}

/// Binds the app's shortcuts, after the kit's.
pub(crate) fn set(cx: &mut App) {
    set_a_macs_own(cx);
    let outside_a_terminal = (!cx.platform_info().is_mac()).then(|| format!("!{}", terminal_view::KEY_CONTEXT));
    if crate::menus::System::THIS.draws_the_menus() {
        // GPUI can bind Alt pressed alone, but sends it for any press of Alt let go with no key between, even over a
        // click, or as Alt-Tab comes back to the window, which would open the menus by surprise.
        cx.bind_keys([key("f10", ToggleMenus, outside_a_terminal.as_deref(), cx)]);
    }
    if let Some(outside_a_terminal) = outside_a_terminal.as_deref() {
        let in_a_terminal = Some(terminal_view::KEY_CONTEXT);
        cx.bind_keys([
            key("ctrl-shift-w", CloseChat, in_a_terminal, cx),
            key("ctrl-shift-n", NewWorkspace, in_a_terminal, cx),
            key("ctrl-shift-q", Quit, in_a_terminal, cx),
            // As Windows and Linux apps have them, besides the keys a Mac has. Bound first, since the kit shows the
            // key bound to an action last, which is then the one a Mac has too.
            key("f3", FindNext, Some(outside_a_terminal), cx),
            key("shift-f3", FindPrevious, Some(outside_a_terminal), cx),
        ]);
    }
    let outside_a_terminal = outside_a_terminal.as_deref();
    let in_settings = Some(settings::KEY_CONTEXT);
    let in_a_prompt = format!("{} > Input", composing::KEY_CONTEXT);
    let bindings = [
        key("secondary-q", Quit, outside_a_terminal, cx),
        key("secondary-,", OpenSettings, outside_a_terminal, cx),
        // Only while Settings shows. Otherwise the same key closes a chat.
        key("escape", CloseSettings, in_settings, cx),
        key("secondary-w", CloseSettings, in_settings, cx),
        key(settings::FIND_KEY, FindSetting, in_settings, cx),
        key("secondary-n", NewWorkspace, outside_a_terminal, cx),
        // Left out of a terminal, where off the Mac Ctrl+B is the program's, as tmux's prefix and back a character in a
        // shell. There the corner's button and the View menu hide it.
        key("secondary-b", ToggleSidebar, outside_a_terminal, cx),
        key("secondary-shift-b", ToggleDiff, None, cx),
        key("space", FlipImage, Some(image_diff::KEY_CONTEXT), cx),
        // As the kit binds them in what's typed in, so the diff's text copies as any other text does.
        key("secondary-c", Copy, Some(file_diff::KEY_CONTEXT), cx),
        key("secondary-a", SelectAll, Some(file_diff::KEY_CONTEXT), cx),
        key("secondary-shift-p", CreatePullRequest, None, cx),
        // Left out of a terminal, whose Select All is Ctrl+Shift+A off the Mac. The terminal's binding is bound last,
        // so it would win anyway, but this keeps that from hanging on the order they're bound in.
        key("secondary-shift-a", ArchiveWorkspace, outside_a_terminal, cx),
        key("secondary-shift-u", MarkWorkspace, None, cx),
        key("secondary-.", StopAgent, outside_a_terminal, cx),
        key("shift-secondary-space", ConversationMode, outside_a_terminal, cx),
        // Back and Forward, as a browser's. The workspace above and below in the sidebar are on the arrows, which the
        // app takes from a text box, where they'd add a cursor above or below.
        key("secondary-[", GoBack, outside_a_terminal, cx),
        key("secondary-]", GoForward, outside_a_terminal, cx),
        key("alt-secondary-up", PreviousWorkspace, outside_a_terminal, cx),
        key("alt-secondary-down", NextWorkspace, outside_a_terminal, cx),
        key("secondary-t", NewChat, outside_a_terminal, cx),
        key("secondary-shift-t", NewTerminal, None, cx),
        key("secondary-w", CloseChat, outside_a_terminal, cx),
        // Shift with a bracket comes as the brace it types, on every system, so that's what's bound. Bound before
        // Ctrl+Tab, so a Mac's menu bar, which shows the first, shows these, since it can't show Tab, and the menus
        // the app draws elsewhere, which show the last, show Ctrl+Tab, as Windows and Linux apps have it.
        key("secondary-}", NextChat, None, cx),
        key("secondary-{", PreviousChat, None, cx),
        key("ctrl-tab", NextChat, None, cx),
        key("ctrl-shift-tab", PreviousChat, None, cx),
        // Bound after the text field's own Shift-Tab, which outdents, so this one wins where a prompt is written.
        key("shift-tab", TogglePlanMode, Some(&in_a_prompt), cx),
        // Left out of a terminal, as the app's other keys on Ctrl and a letter are, though Find is a chat's, so it's
        // never there to take them from the program.
        key("secondary-f", FindInChat, outside_a_terminal, cx),
        key("secondary-g", FindNext, outside_a_terminal, cx),
        key("secondary-shift-g", FindPrevious, None, cx),
    ];
    cx.bind_keys(bindings);
    for bind in [bind_selecting, bind_scrolling, dictation_ui::bind_keys, terminal_view::bind_keys] {
        bind(cx);
    }
}

/// The keys for what's selected in a chat, a rendered Markdown file or a proposal that has focus, which a press on its
/// words gives it. Select All takes all of its words, with the keys the kit selects all with, and copies with the keys
/// it copies with, from its own binding. The others move the end of what's selected that moved last, with the keys and
/// actions the kit's text boxes move theirs with.
fn bind_selecting(cx: &mut App) {
    let focused = Some(transcript::KEY_CONTEXT);
    // A word at a time is Option on a Mac, as in its text boxes, and Ctrl elsewhere, as in theirs.
    let by_word = if cx.platform_info().is_mac() { "alt-shift" } else { "ctrl-shift" };
    cx.bind_keys([
        key("secondary-a", SelectAll, focused, cx),
        key("shift-left", SelectLeft, focused, cx),
        key("shift-right", SelectRight, focused, cx),
        key(&format!("{by_word}-left"), SelectToPreviousWordStart, focused, cx),
        key(&format!("{by_word}-right"), SelectToNextWordEnd, focused, cx),
        key("shift-up", SelectUp, focused, cx),
        key("shift-down", SelectDown, focused, cx),
    ]);
}

/// The keys that scroll a chat, a rendered Markdown file or a proposal that has focus, as they'd scroll a page, with the
/// actions a text box moves its cursor with, which scroll a view that has no cursor, as `transcript` takes them. Page
/// Up and Page Down scroll a chat from its composer and a question in its place too, and Escape goes back from the chat
/// to the composer. `Option-Up` and `Option-Down`, `Alt` off the Mac, step a chat between its prompts, from the
/// composer too, a rendered Markdown file between its headings, and a proposal between its parts, as what holds each
/// takes them, and with Shift, they move the end of what's selected to the same places.
fn bind_scrolling(cx: &mut App) {
    let focused = Some(transcript::KEY_CONTEXT);
    let around_a_chat = Some(CHAT_KEY_CONTEXT);
    // After the kit's own, in the composer and a question's row for typing an answer, so these win there, and in Find's
    // bar, which, as a text box of one line, lets Up and Down go on to the chat too.
    let typed_around_a_chat = format!("{CHAT_KEY_CONTEXT} > Input");
    let typed_around_a_chat = Some(typed_around_a_chat.as_str());
    let in_a_chat = format!("{CHAT_KEY_CONTEXT} > {}", transcript::KEY_CONTEXT);
    cx.bind_keys([
        key("up", MoveUp, focused, cx),
        key("down", MoveDown, focused, cx),
        key("pageup", MovePageUp, focused, cx),
        key("pagedown", MovePageDown, focused, cx),
        key("home", MoveToStart, focused, cx),
        key("end", MoveToEnd, focused, cx),
        key("pageup", ChatPageUp, around_a_chat, cx),
        key("pagedown", ChatPageDown, around_a_chat, cx),
        key("pageup", ChatPageUp, typed_around_a_chat, cx),
        key("pagedown", ChatPageDown, typed_around_a_chat, cx),
        key("escape", BackToComposer, Some(&in_a_chat), cx),
        key("alt-up", StepBack, focused, cx),
        key("alt-down", StepOn, focused, cx),
        key("alt-up", StepBack, around_a_chat, cx),
        key("alt-down", StepOn, around_a_chat, cx),
        key("alt-shift-up", SelectStepBack, focused, cx),
        key("alt-shift-down", SelectStepOn, focused, cx),
    ]);
}

/// `keys` doing `action` where `context` says, or wherever focus is. GPUI reads `secondary` only in a binding it
/// loads for the system it's running on, rather than one it makes from a fixed string.
fn key(keys: &str, action: impl Action, context: Option<&str>, cx: &App) -> KeyBinding {
    // Each is a constant in the app's code, so one that doesn't parse is a mistake in it, as `KeyBinding::new` takes
    // it to be.
    let context = context.map(|context| match KeyBindingContextPredicate::parse(context) {
        Ok(predicate) => Rc::new(predicate),
        Err(e) => panic!("the key context {context:?} doesn't parse: {e}"),
    });
    match KeyBinding::load(keys, Box::new(action), context, false, None, cx) {
        Ok(binding) => binding,
        Err(e) => panic!("the keys {keys:?} don't parse: {e}"),
    }
}
