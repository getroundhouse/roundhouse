//! The issues and pull requests chats name, which the hub looks up on GitHub for every client and keeps in memory, so
//! two clients showing one chat ask GitHub once, and a client asks again for those it shows as often as it likes.

mod kept;
mod reading;
#[cfg(test)]
mod tests;

pub(super) use kept::Kept;
