//! Menus: how every menu is drawn, its rows as tall as a large control, with a card's corners around them, as
//! `docs/spec/components.md` says. The kit's menu draws them, from the style set here once for the whole app.

#[cfg(test)]
mod tests;

mod style;
pub(crate) use style::style;
