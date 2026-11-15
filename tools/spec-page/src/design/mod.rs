//! The design system's page, beside the spec's: the foundations `look.md` names, laid out to look one up by its name,
//! its value or what it's for, and then the components `components.md` has, which are built on them. Each of the
//! tables whose caption names a type in `look`, such as `look::Space`, shows each name in it beside its own piece of
//! the gallery's sheet of that type, cut out where the gallery says it drew it. The rest of each file, its rules and
//! its other screens, is under the heading it's under, as it is on the spec's page. It shows nothing the spec
//! doesn't, and it's in the app's own colours, from the spec's table of them, so each piece of a sheet sits on the
//! colour it was drawn on.

#[cfg(test)]
mod tests;

mod html;

#[cfg(test)]
use html::draw;
pub(crate) use html::write;
