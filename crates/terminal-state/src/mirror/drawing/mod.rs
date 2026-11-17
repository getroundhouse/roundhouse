//! Copies what a mirror shows into a [`Frame`], from Ghostty's render state, building again only the rows that
//! changed since the last copy.

mod copying;
#[cfg(test)]
mod tests;

pub(super) use copying::Drawing;
