//! Signing in: the page the person approves on, with PKCE, a state and the server the tokens are for, and what came
//! back from it, checked, for tokens.

#[cfg(test)]
mod tests;

mod beginning;
#[cfg(test)]
use beginning::challenge_of;
pub use beginning::{Pending, Started};
mod finishing;
pub use finishing::SignedIn;
mod redirect;
pub use redirect::{NotARedirect, RedirectUri};
mod returned;
pub use returned::Returned;
