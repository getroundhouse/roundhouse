//! What the program in a terminal reads for a key, a paste or a turn of the wheel, by the modes it has set, which only
//! a copy of what it wrote knows.

mod encoding;
#[cfg(test)]
mod tests;

pub(super) use encoding::Typing;
