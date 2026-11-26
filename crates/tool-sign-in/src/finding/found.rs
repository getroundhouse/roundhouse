use protocol::client::tool_sign_ins::NoToolSignIn;

use super::Way;

/// Whether a server takes a sign-in.
#[derive(Debug)]
pub enum Found {
    /// It answers anyone, and says of no sign-in.
    NoSignIn,
    /// It asks for a sign-in, or says of one, and has no way that Roundhouse could give it one. It may take a key.
    NoWay(NoToolSignIn),
    /// It asks for a sign-in, or answers anyone and has one.
    SignIn(Box<Way>),
}
