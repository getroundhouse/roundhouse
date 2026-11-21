//! Key release crosses view and dialog boundaries, and reaches the one active field. Repeats are swallowed globally
//! once Space is held to dictate, even if Option is released first or a dialog takes focus meanwhile. Space held goes
//! on doing what it was while another app is in front, which hears it let go of, so the system asks then.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, Context, Empty, Global, InteractiveElement as _, IntoElement, KeyBinding,
    KeyBindingContextPredicate, KeyUpEvent, KeystrokeEvent, ParentElement as _, Render, Styled as _, Task, Window, div,
};

use crate::{keyboard, recording::Started, source};

gpui_kit::actions!(dictation, [HoldToDictate, CancelDictation]);

pub const HOLD_TO_DICTATE_KEY: &str = "alt-space";
pub(crate) const SPACE: &str = "space";
pub(crate) const CONTEXT: &str = "DictationField";

/// How often the system is asked whether Space is still down while another app is in front: soon enough after it's
/// let go of that what was said goes in as it would have.
const ASK_AGAIN_AFTER: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Held(bool);
impl Global for Held {}

/// Asks the system whether Space is still down while another app is in front, until it isn't or the app comes back.
#[derive(Default)]
struct Away(Option<Task<()>>);
impl Global for Away {}

pub(crate) fn held(cx: &mut App) {
    cx.default_global::<Held>().0 = true;
}

/// Bound after the input kit's keys, so Escape cancels a recording before it leaves a field or closes a dialog.
pub fn bind_keys(cx: &mut App) {
    if !sound::HEARS {
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

/// Space held to dictate when another app comes in front goes on dictating, or towards it, as when the system shows
/// something in front for a moment, until the system says it's been let go of, which only the app in front hears.
/// Otherwise what's dictated in `window` stops as letting go does, so the microphone isn't left on with nobody looking,
/// even with Space down for something else, such as ⌘Space. Space held is forgotten as the app comes back if the
/// system says it's up, since no key reaches the app while it's behind. Where the system won't say, it's kept until
/// Space's key-up, so its repeats don't type: the cost is one press taken for a release the app never heard.
pub fn window_activation_changed(window: AnyWindowHandle, active: bool, cx: &mut App) {
    cx.default_global::<Away>().0 = None;
    if active {
        if keyboard::space_is_down(cx) == Some(false) {
            let_go(cx);
        }
    } else if holding_space(window, cx) {
        let asking = cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(ASK_AGAIN_AFTER).await;
                if cx.update(|cx| keyboard::space_is_down(cx)) != Some(true) {
                    return cx.update(let_go);
                }
            }
        });
        cx.default_global::<Away>().0 = Some(asking);
    } else if let Some(field) = source::active(cx) {
        let _ = field.update(cx, |field, cx| {
            if field.window == window {
                field.space_down = None;
                field.stop(cx);
            }
        });
    }
}

/// Whether Space is held to dictate, or towards it, in `window`, as the keys the app heard say and the system says is
/// still so.
fn holding_space(window: AnyWindowHandle, cx: &App) -> bool {
    let towards = source::active(cx)
        .and_then(|field| field.upgrade())
        .is_some_and(|field| field.read(cx).window == window && field.read(cx).space_down.is_some());
    let held = cx.try_global::<Held>().is_some_and(|held| held.0);
    (held || towards) && keyboard::space_is_down(cx) == Some(true)
}

/// Space let go of: what holding it started stops, and it types again.
fn let_go(cx: &mut App) {
    cx.default_global::<Held>().0 = false;
    if let Some(active) = source::active(cx) {
        let _ = active.update(cx, |field, cx| {
            field.space_down = None;
            if field.underway.as_ref().is_some_and(|u| u.started == Started::Key) {
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
                if event.keystroke.key == SPACE {
                    let_go(cx);
                }
            })
            .child(surface)
    }
}
