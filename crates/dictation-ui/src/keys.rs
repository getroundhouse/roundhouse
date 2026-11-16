//! Key release crosses view and dialog boundaries, and reaches the one active field. Repeats are swallowed globally
//! once Space is held to dictate, even if Option is released first or a dialog takes focus meanwhile.

use std::rc::Rc;

use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, Context, Empty, Global, InteractiveElement as _, IntoElement, KeyBinding,
    KeyBindingContextPredicate, KeyUpEvent, KeystrokeEvent, ParentElement as _, Render, Styled as _, Window, div,
};

use crate::{recording::Started, source};

gpui_kit::actions!(dictation, [HoldToDictate, CancelDictation]);

pub const HOLD_TO_DICTATE_KEY: &str = "alt-space";
pub(crate) const SPACE: &str = "space";
pub(crate) const CONTEXT: &str = "DictationField";

#[derive(Default)]
struct Held(bool);
impl Global for Held {}

pub(crate) fn held(cx: &mut App) {
    cx.default_global::<Held>().0 = true;
}

/// Bound after the input kit's keys, so Escape cancels a recording before it leaves a field or closes a dialog.
pub fn bind_keys(cx: &mut App) {
    if !dictation::HEARS {
        return;
    }
    let context = match KeyBindingContextPredicate::parse(CONTEXT) {
        Ok(context) => context,
        Err(error) => panic!("invalid dictation context: {error}"),
    };
    let context = Some(Rc::new(context));
    let bindings = [
        (HOLD_TO_DICTATE_KEY, Box::new(HoldToDictate) as Box<dyn gpui_kit::Action>),
        ("escape", Box::new(CancelDictation)),
        ("alt-escape", Box::new(CancelDictation)),
    ]
    .map(|(key, action)| match KeyBinding::load(key, action, context.clone(), false, None, cx) {
        Ok(binding) => binding,
        Err(error) => panic!("invalid dictation key {key}: {error}"),
    });
    cx.bind_keys(bindings);
}

pub fn window_left(window: AnyWindowHandle, cx: &mut App) {
    cx.default_global::<Held>().0 = false;
    if let Some(active) = source::active(cx) {
        let _ = active.update(cx, |field, cx| {
            if field.window == window {
                field.space_down = None;
                field.stop(cx);
            }
        });
    }
}

pub(crate) fn install(cx: &mut App) {
    cx.intercept_keystrokes(|event: &KeystrokeEvent, _: &mut Window, cx: &mut App| {
        if event.keystroke.key == SPACE && cx.try_global::<Held>().is_some_and(|held| held.0) {
            cx.stop_propagation();
        }
    })
    .detach();
    Root::register_plugin::<Releases>(cx, |_, _| Releases);
}

struct Releases;
impl Render for Releases {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}
impl RootPlugin for Releases {
    fn decorate(&self, surface: AnyElement, _: &Root, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .size_full()
            .capture_key_up(|event: &KeyUpEvent, _, cx| {
                if event.keystroke.key != SPACE {
                    return;
                }
                cx.default_global::<Held>().0 = false;
                if let Some(active) = source::active(cx) {
                    let _ = active.update(cx, |field, cx| {
                        field.space_down = None;
                        if field.underway.as_ref().is_some_and(|u| u.started == Started::Key) {
                            field.stop(cx);
                        }
                    });
                }
            })
            .child(surface)
    }
}
