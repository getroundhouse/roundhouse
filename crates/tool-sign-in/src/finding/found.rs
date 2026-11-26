use thiserror::Error;

use super::{Issuer, Way};
use crate::OauthError;
use crate::failure::shown;

/// Whether a server takes a sign-in.
#[derive(Debug)]
pub enum Found {
    /// It answers anyone, and says of no sign-in.
    NoSignIn,
    /// It asks for a sign-in, or says of one, and has no way that Roundhouse could give it one. It may take a key.
    NoWay(NoWay),
    /// It asks for a sign-in, or answers anyone and has one.
    SignIn(Box<Way>),
}

/// Why a server has no way that Roundhouse could give it a sign-in.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum NoWay {
    /// It names no authorization server, or one that doesn't say how to sign in.
    #[error("the server asks for a sign-in and doesn't say where to sign in")]
    NoAuthorizationServer,
    /// Its authorization server takes only clients registered with it, and has nowhere to register.
    #[error("the authorization server {} takes only clients registered with it", shown(.0.as_str()))]
    OnlyRegisteredClients(Issuer),
    /// Its authorization server takes only clients with a secret, and Roundhouse, whose code anyone has, can keep none.
    #[error("the authorization server {} takes only clients with a secret", shown(.0.as_str()))]
    OnlyClientsWithSecrets(Issuer),
    /// Its authorization server wouldn't register Roundhouse, for this of OAuth's errors where it said one.
    #[error("the authorization server {} wouldn't register Roundhouse", shown(issuer.as_str()))]
    NotRegistered { issuer: Issuer, error: Option<OauthError> },
    /// What it says of its sign-in is said of another server, at `host`, where what it said is an address.
    #[error("the server says its sign-in is for {}, which isn't its address", .host.as_deref().unwrap_or("another"))]
    OtherResource { host: Option<String> },
    /// Its authorization server doesn't say it takes PKCE with S256, which MCP has a client refuse.
    #[error("the authorization server {} doesn't say it takes PKCE with S256", shown(.0.as_str()))]
    NoS256(Issuer),
}
