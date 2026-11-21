//! Tooltips: the few words that say what something is or does while the pointer rests on it, with the key that does the
//! same, if there's one. What says more, such as a path or a problem's detail, is a hover card's. Every tooltip is the
//! kit's, built by the kit's own parts as well as by `look`'s, so `look` sets how every one is drawn, once, with
//! [`style`], which `crate::apply` installs.

#[cfg(test)]
mod tests;

mod style;
mod tooltip;
pub(crate) use style::style;
#[cfg(any(test, feature = "testing"))]
pub use tooltip::shown_tooltip;
pub use tooltip::{Tooltipped, tooltip_words};
