//! The app's shortcuts. Each that a Mac has on Cmd is bound with `secondary`, which is Cmd on a Mac and Ctrl elsewhere,
//! since off the Mac `cmd` is the Windows or Super key, which the system keeps for itself.
//!
//! A shortcut bound with no context takes its key wherever focus is, even from a view that binds the same key deeper
//! in, since GPUI ranks a binding without a context with the deepest. Where both act, the app's wins, as it's bound
//! after the kit's, and GPUI goes on to the view's only where nothing takes the app's. So the kit's text boxes keep
//! their own Ctrl keys only because none of their editing keys is one of these that has no context. The four they
//! share, Ctrl with F, [, ] and ., which a text box binds to search, outdent, indent and its code actions, the app
//! takes from them on a Mac too, with Cmd. A view that needs one of these for itself, as a terminal does Ctrl+W, has
//! to be left out of where it's bound.

use std::rc::Rc;

use gpui_kit::component::input::{Copy, SelectAll};
use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate};

use crate::{
    ArchiveWorkspace, CloseChat, CloseSettings, CreatePullRequest, FindInChat, FindNext, FindPrevious, FindSetting,
    FlipImage, Hide, HideOthers, MarkWorkspace, Minimize, NewChat, NewWorkspace, NextChat, NextWorkspace, OpenSettings,
    PreviousChat, PreviousWorkspace, Quit, StopAgent, ToggleDiff, ToggleMenus, TogglePlanMode, composing, file_diff,
    image_diff, settings,
};

/// Binds the app's shortcuts, after the kit's.
pub(crate) fn set(cx: &mut App) {
    if cx.platform_info().is_mac() {
        // A Mac's own. Only a Mac hides an app, and elsewhere the system minimizes a window itself.
        cx.bind_keys([
            key("secondary-h", Hide, None, cx),
            key("alt-secondary-h", HideOthers, None, cx),
            key("secondary-m", Minimize, None, cx),
        ]);
    } else {
        // As Windows and Linux apps have them, besides the keys a Mac has. Bound first, since the kit shows the key
        // bound to an action last, which is then the one a Mac has too.
        cx.bind_keys([key("f3", FindNext, None, cx), key("shift-f3", FindPrevious, None, cx)]);
    }
    if crate::menus::System::THIS.draws_the_menus() {
        // GPUI can bind Alt pressed alone, but sends it for any press of Alt let go with no key between, even over a
        // click, or as Alt-Tab comes back to the window, which would open the menus by surprise.
        cx.bind_keys([key("f10", ToggleMenus, None, cx)]);
    }
    let in_settings = Some(settings::KEY_CONTEXT);
    let in_a_prompt = format!("{} > Input", composing::KEY_CONTEXT);
    let bindings = [
        key("secondary-q", Quit, None, cx),
        key("secondary-,", OpenSettings, None, cx),
        // Only while Settings shows. Otherwise the same key closes a chat.
        key("escape", CloseSettings, in_settings, cx),
        key("secondary-w", CloseSettings, in_settings, cx),
        key(settings::FIND_KEY, FindSetting, in_settings, cx),
        key("secondary-n", NewWorkspace, None, cx),
        key("secondary-shift-d", ToggleDiff, None, cx),
        key("space", FlipImage, Some(image_diff::KEY_CONTEXT), cx),
        // As the kit binds them in what's typed in, so the diff's text copies as any other text does.
        key("secondary-c", Copy, Some(file_diff::KEY_CONTEXT), cx),
        key("secondary-a", SelectAll, Some(file_diff::KEY_CONTEXT), cx),
        key("secondary-shift-p", CreatePullRequest, None, cx),
        key("secondary-shift-a", ArchiveWorkspace, None, cx),
        key("secondary-shift-u", MarkWorkspace, None, cx),
        key("secondary-.", StopAgent, None, cx),
        key("secondary-]", NextWorkspace, None, cx),
        key("secondary-[", PreviousWorkspace, None, cx),
        key("secondary-t", NewChat, None, cx),
        key("secondary-w", CloseChat, None, cx),
        key("ctrl-tab", NextChat, None, cx),
        key("ctrl-shift-tab", PreviousChat, None, cx),
        // Bound after the text field's own Shift-Tab, which outdents, so this one wins where a prompt is written.
        key("shift-tab", TogglePlanMode, Some(&in_a_prompt), cx),
        key("secondary-f", FindInChat, None, cx),
        key("secondary-g", FindNext, None, cx),
        key("secondary-shift-g", FindPrevious, None, cx),
        // Selects all of the agent's words in a chat that has focus, which a press on them gives it, with the keys the
        // kit selects all with, and copies with the keys it copies with, from its own binding.
        key("secondary-a", SelectAll, Some(transcript::KEY_CONTEXT), cx),
    ];
    cx.bind_keys(bindings);
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
