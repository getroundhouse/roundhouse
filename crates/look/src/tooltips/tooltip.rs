use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyView, App, InteractiveElement as _, ParentElement as _, SharedString, StatefulInteractiveElement,
    TestSupportExt as _, Window, div,
};

use crate::traits::Input;

/// Says a few words while the pointer rests on something that isn't one of `look`'s components, which say theirs
/// themselves, such as a button's. A few words, as `docs/spec/components.md` says, since what says more is a hover
/// card's.
pub trait Tooltipped: StatefulInteractiveElement + Sized {
    /// Says `words` while the pointer rests on it: `.says("Stop", cx)`. Under touch it says nothing, since nothing rests
    /// on it, and GPUI would show it on a long press, which is what opens a menu there.
    #[must_use]
    fn says(self, words: impl Into<SharedString>, cx: &App) -> Self {
        match Input::of(cx) {
            Input::Pointer => self.tooltip(tooltip(words)),
            Input::Touch => self,
        }
    }

    /// Shows what `preview` builds while the pointer rests on it, in a tooltip the pointer can move onto, such as an
    /// edit's diff. Under touch it shows none, since nothing rests on it, and GPUI would show it on a long press, which
    /// is what opens a menu there. A tap shows the same some other way, such as by opening what it's on.
    #[must_use]
    fn previews(self, preview: impl Fn(&mut Window, &mut App) -> AnyView + 'static, cx: &App) -> Self {
        match Input::of(cx) {
            Input::Pointer => self.hoverable_tooltip(preview),
            Input::Touch => self,
        }
    }
}

impl<T: StatefulInteractiveElement> Tooltipped for T {}

/// What a tooltip on one of the kit's components says, which takes its words rather than a tooltip: `words` with a
/// pointer, and nothing under touch, as [`Tooltipped::says`] says.
pub fn tooltip_words(words: impl Into<SharedString>, cx: &App) -> Option<SharedString> {
    (Input::of(cx) == Input::Pointer).then(|| words.into())
}

/// A tooltip that says `words`. A test finds what it says as `tooltip-words`, since only one tooltip shows at a time.
pub(crate) fn tooltip(words: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let words = words.into();
    move |window, cx| {
        let words = words.clone();
        Tooltip::element(move |_, _| {
            div().id("tooltip-words").aria_label(words.clone()).child(words.clone()).test_support()
        })
        .build(window, cx)
    }
}

/// A tooltip that says `words`, with the key `binding` names, if there's one, drawn where it's put rather than under the
/// pointer, for a sheet of how tooltips look.
#[cfg(any(test, feature = "testing"))]
pub fn shown_tooltip(words: &'static str, binding: Option<&'static str>, window: &mut Window, cx: &mut App) -> AnyView {
    Tooltip::new(words).key_binding(binding.and_then(crate::keys::kit_key)).build(window, cx)
}
