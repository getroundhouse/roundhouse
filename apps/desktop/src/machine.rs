//! What the app asks of the machine it runs on, which it's given as a value, so a test can give it another machine's.

use std::sync::Arc;

use gpui_kit::{App, Global};

use crate::app::{TitleBar, computer_name};
use crate::lost_release::{Button, left_button};

/// The machine the app runs on, as far as the app asks of it. An app given none has [`Machine::this_one`].
#[derive(Clone)]
pub(crate) struct Machine {
    /// Who moves the window from the strip along its top.
    pub(crate) title_bar: TitleBar,
    /// Where the left mouse button is now.
    pub(crate) left_button: fn() -> Button,
    /// What the machine calls itself, read off the main thread.
    pub(crate) name: Arc<dyn Fn() -> String + Send + Sync>,
}

impl Global for Machine {}

impl Machine {
    /// The one this process runs on.
    pub(crate) fn this_one() -> Self {
        Self { title_bar: TitleBar::OF_THE_SYSTEM, left_button, name: Arc::new(computer_name) }
    }

    /// The one the app was given.
    pub(crate) fn of(cx: &App) -> Self {
        cx.try_global::<Self>().cloned().unwrap_or_else(Self::this_one)
    }
}

/// The app's machine, for a test to change.
#[cfg(any(test, feature = "testing"))]
pub(crate) fn given(cx: &mut App) -> &mut Machine {
    if !cx.has_global::<Machine>() {
        cx.set_global(Machine::this_one());
    }
    cx.global_mut::<Machine>()
}
