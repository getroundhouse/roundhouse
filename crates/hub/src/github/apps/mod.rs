//! Apps of your own, as GitHub makes and answers for them: the manifest GitHub's manifest flow makes one from, making
//! the code GitHub sends the person back with into the app's key, its bot, and, asked as the app with a JWT its key
//! signs, where it's installed and a token for each installation.

#[cfg(any(test, feature = "testing"))]
pub(crate) use key::Claims;
#[cfg(test)]
mod tests;

mod installations;
pub(crate) use installations::{Installation, Minted};
mod key;
pub(crate) use key::GithubAppKey;
mod made;
pub(crate) use made::MadeApp;
mod manifest;
pub(crate) use manifest::manifest;
