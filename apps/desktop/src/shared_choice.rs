//! A choice every workspace shares until the app quits, such as how the file tab shows a diff, kept by its type, and
//! the switch that makes one where its choices are a switch's buttons.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::{AnyElement, App, Global, IntoElement as _};

/// The choice of a `T` made last, which every workspace shares until the app quits.
pub(crate) struct Chosen<T>(T);

impl<T: 'static> Global for Chosen<T> {}

/// The `T` chosen last, or the default, before any has been.
pub(crate) fn chosen<T: Copy + Default + 'static>(cx: &App) -> T {
    cx.try_global::<Chosen<T>>().map_or_default(|chosen| chosen.0)
}

/// Chooses `choice`, unless it's chosen, so what's told of a choice hears only of one that changed it.
pub(crate) fn choose<T: Copy + Default + PartialEq + 'static>(choice: T, cx: &mut App) {
    if chosen::<T>(cx) != choice {
        cx.set_global(Chosen(choice));
    }
}

/// A choice made on a switch on a track, a button with an icon to each.
pub trait OnASwitch: Copy + Default + PartialEq + 'static {
    /// In the order the switch shows them.
    const ALL: &'static [Self];

    /// The ID of its button on the switch.
    fn id(self) -> &'static str;

    fn name(self) -> &'static str;

    fn icon(self) -> Lucide;
}

/// The choices of a `T`, as one switch on a track, with the one chosen on.
pub(crate) fn switch<T: OnASwitch>(cx: &App) -> AnyElement {
    let showing = chosen::<T>(cx);
    (T::ALL.iter().copied())
        .fold(look::switch(look::SwitchKind::Tracked), |switch, choice| {
            switch.icon(choice.id(), choice.icon(), choice.name(), choice == showing, move |_, cx| choose(choice, cx))
        })
        .into_any_element()
}
