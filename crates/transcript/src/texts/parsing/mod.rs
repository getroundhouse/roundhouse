//! When and where a text is parsed: where it's drawn, when what's to be parsed of it is short, and otherwise on another
//! thread, as [`Texts::parsed`](super::Texts::parsed) says.

#[cfg(test)]
mod tests;

mod apart;
pub(super) use apart::Apart;
