//! Keys: a key to press, shown where it's pressed, such as the key that finds a setting beside Settings' search, or
//! Return after the words of the button it presses. `docs/spec/components.md` has the table of its kinds, which a test
//! holds to these.

#[cfg(test)]
mod tests;

mod key;
mod kinds;
pub use key::{Key, key};
pub use kinds::KeyKind;
