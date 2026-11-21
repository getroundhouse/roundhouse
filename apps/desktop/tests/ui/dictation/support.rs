//! Making dictation ready, the key and what's said, and what the composer shows of it.

use std::path::Path;

use desktop::dictating::STATUS;
use desktop::workspace_pane::LIVE_LINE;
use dictation::EngineError;
use dictation::fake::FakeEngine;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, KeyDownEvent, KeyUpEvent, Keystroke, PlatformInput};
use models::ModelFiles;
use protocol::event::EventBody;
use protocol::model::SessionId;

use crate::support::{Stack, Ui};

pub(super) const SCRIPT: &[&str] = &["fix", "the", "flaky", "test", "in", "hub"];

/// Makes dictation ready, with a stand-in for the model that hears `SCRIPT`.
pub(super) fn ready(ui: &mut Ui) -> FakeEngine {
    ready_with(ui, FakeEngine::new(SCRIPT))
}

pub(super) fn ready_with(ui: &mut Ui, engine: FakeEngine) -> FakeEngine {
    let ready = engine.clone();
    ui.cx.update(|cx| desktop::dictating::use_engine(ready, cx));
    ui.wait("dictation to be ready", |ui| ui.cx.update(|cx| desktop::dictating::is_ready(cx)));
    engine
}

/// What the box under Dictation in Settings says, or the microphone's popover, while there's something to say.
pub(super) fn status(ui: &mut Ui) -> Option<String> {
    ui.frame();
    let said = |window: &gpui_kit::Window| window.try_find(STATUS).and_then(|l| l.label().map(str::to_string));
    ui.cx.update_window(ui.handle, |_, window, _| said(window)).unwrap()
}

pub(super) fn says(ui: &mut Ui, start: &str) {
    ui.wait(&format!("dictation to say {start:?}"), |ui| status(ui).is_some_and(|said| said.starts_with(start)));
}

/// Chooses `choice` for Dictation in Settings, which is open.
pub(super) fn choose(ui: &mut Ui, choice: &str) {
    ui.pick_item("settings-dictation", choice);
}

/// Has dictation use `files`, kept in `model_dir`, from `from`, and loaded with `load`, from the folder it's given.
pub(super) fn use_model(
    ui: &mut Ui,
    model_dir: &Path,
    files: ModelFiles,
    from: &str,
    load: impl Fn(&Path) -> Result<FakeEngine, EngineError> + Send + Sync + 'static,
) {
    let (model_dir, from) = (model_dir.to_path_buf(), from.to_string());
    ui.cx.update(|cx| desktop::dictating::use_model(model_dir, files, from, load, cx));
}

fn key(ui: &mut Ui, event: PlatformInput) {
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event, cx)).unwrap();
    ui.frame();
}

/// Presses `Option-Space`, which dictates at once while it's held.
pub(super) fn hold_option_space(ui: &mut Ui) {
    let keystroke = Keystroke::parse("alt-space").unwrap();
    key(ui, PlatformInput::KeyDown(KeyDownEvent { keystroke, is_held: false, prefer_character_input: false }));
}

/// Has the system say Space is down, or up, whichever keys the app has been given.
pub(super) fn on_the_keyboard(ui: &mut Ui, space_down: bool) {
    ui.cx.update(|cx| dictation_ui::hold_space_down(space_down, cx));
}

/// Lets go of Space.
pub(super) fn let_go(ui: &mut Ui) {
    key(ui, PlatformInput::KeyUp(KeyUpEvent { keystroke: Keystroke::parse("space").unwrap() }));
}

/// `keys` pressed as the system gives them, typing `typed` where no binding takes them.
pub(super) fn type_keys(ui: &mut Ui, keys: &'static str, typed: &str) {
    let mut keystroke = Keystroke::parse(keys).unwrap();
    keystroke.key_char = Some(typed.to_string());
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_keystroke(keystroke, cx)).unwrap();
    ui.frame();
}

/// Presses Space, as the system gives it, typing a space where nothing takes it.
pub(super) fn press_space(ui: &mut Ui) {
    type_keys(ui, "space", " ");
}

/// Space, repeated by the system while it's held. Whether the app took it, since the system types a space only where
/// nothing does.
pub(super) fn repeat_space(ui: &mut Ui) -> bool {
    let mut keystroke = Keystroke::parse("space").unwrap();
    keystroke.key_char = Some(" ".to_string());
    let event = PlatformInput::KeyDown(KeyDownEvent { keystroke, is_held: true, prefer_character_input: false });
    let dispatched = ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event, cx)).unwrap();
    ui.frame();
    !dispatched.propagate
}

/// Whether anything is being dictated into the pane showing.
pub(super) fn dictating(ui: &mut Ui) -> bool {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, desktop::workspace_pane::WorkspacePane::dictating)
}

/// What's said, given to what's being dictated, wherever it is.
pub(super) fn say(ui: &mut Ui, engine: &FakeEngine, said: &str) {
    hear(ui, &engine.saying(said));
}

/// `audio`, given to what's being dictated, wherever it is, since one field dictates at a time.
pub(super) fn hear(ui: &mut Ui, audio: &[f32]) {
    ui.cx.update(|cx| dictation_ui::hear(audio, cx));
}

/// The live text drawn under what's typed, while something is being dictated.
pub(super) fn live(ui: &mut Ui) -> Option<String> {
    ui.frame();
    let line = |window: &gpui_kit::Window| window.try_find(LIVE_LINE).and_then(|l| l.label().map(str::to_string));
    ui.cx.update_window(ui.handle, |_, window, _| line(window)).unwrap()
}

/// Waits for the live text to start with `start`.
pub(super) fn hearing(ui: &mut Ui, start: &str) {
    ui.wait(&format!("the live text to start {start:?}"), |ui| live(ui).is_some_and(|text| text.starts_with(start)));
}

/// Waits for recording and finalization to end, including when nothing was heard.
pub(super) fn ended(ui: &mut Ui) {
    ui.wait("the recording controls to go", |ui| !ui.has(dictation_ui::STOP));
}

/// Whether a prompt has been sent in the chat.
pub(super) fn sent(ui: &Ui) -> bool {
    ui.events().iter().any(|e| matches!(e.body, EventBody::UserPrompt { .. }))
}

/// The draft the hub keeps for `chat`.
pub(super) fn kept(stack: &Stack, chat: &SessionId) -> String {
    stack.client().session_get(chat).unwrap().draft.text
}
