use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyView, App, InteractiveElement as _, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    TestSupportExt as _, Window, div,
};

/// A tooltip that says `words`, for what isn't one of `look`'s components, which say theirs themselves, such as a
/// button's: `.tooltip(look::tooltip("Stop"))`. A few words, as `docs/spec/components.md` says, since what says more is
/// a hover card's. A test finds what it says as `tooltip-words`, since only one tooltip shows at a time.
pub fn tooltip(words: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
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
