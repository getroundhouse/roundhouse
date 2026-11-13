//! The page: each area, rule by rule, each beside the screens that show it, and a switch that leaves only what the
//! owner hasn't confirmed, so an area can be confirmed at one sitting. It shows only what the index holds, and links
//! each image to itself at full size.
//!
//! The page's folder is as deep in the repo as `docs/spec/`, and the spec's screens are copied into it, so what the
//! spec points to, such as `screens/chat.png` or the app's icon in `../../apps/`, is found from the page as it's
//! written.

#[cfg(test)]
mod tests;

mod html;

#[cfg(test)]
pub(crate) use html::draw;
pub(crate) use html::write;
