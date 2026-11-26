use protocol::model::{ToolScopes, ToolServerId, ToolServerName, ToolServerPlace, ToolServerUrl};
use tool_sign_in::{Issuer, Registration};

use super::editing::Changes;
use super::kept::KeptSignIn;
use crate::login_files::LoginFile;

/// What a sign-in is to, as it started.
#[derive(Debug)]
pub(crate) struct SigningIn {
    pub(super) to: To,
    pub(super) url: ToolServerUrl,
    pub(super) before: Option<Before>,
}

/// The sign-in a server had as it's signed in to again.
#[derive(Debug)]
pub(super) struct Before {
    issuer: Issuer,
    /// What signing in again asks `issuer` to allow.
    asks: ToolScopes,
    registration: Option<Registration>,
}

impl Before {
    pub(super) fn of(kept: KeptSignIn) -> Self {
        let (issuer, asks) = (kept.renewable.issuer().clone(), kept.to_ask_for());
        Self { issuer, asks, registration: kept.registration }
    }

    /// The same, of a server that's signed in to as another.
    pub(super) fn moved(self) -> Self {
        Self { asks: ToolScopes::default(), ..self }
    }
}

#[derive(Debug)]
pub(super) enum To {
    New {
        id: ToolServerId,
        name: ToolServerName,
    },
    /// The server whose file this is, which no other server's ever is.
    Again {
        id: ToolServerId,
        file: LoginFile,
    },
    /// One to make `changes` to, which was at `at` as the sign-in started, with this file if it named one.
    Edit {
        id: ToolServerId,
        changes: Changes,
        at: ToolServerPlace,
        file: Option<LoginFile>,
    },
}

impl To {
    /// The server there is that it's to.
    pub(super) fn there(&self) -> Option<&ToolServerId> {
        match self {
            To::New { .. } => None,
            To::Again { id, .. } | To::Edit { id, .. } => Some(id),
        }
    }

    /// The file of that server's that a sign-in before would be in.
    pub(super) fn file(&self) -> Option<&LoginFile> {
        match self {
            To::New { .. } => None,
            To::Again { file, .. } => Some(file),
            To::Edit { file, .. } => file.as_ref(),
        }
    }
}

impl SigningIn {
    pub(crate) fn url(&self) -> &ToolServerUrl {
        &self.url
    }

    /// What the sign-in before was asked for and granted, and its server has wanted since, which `issuer` is to be
    /// asked to allow too, as [`KeptSignIn::to_ask_for`] says. What's of another authorization server is nothing to
    /// this one.
    pub(crate) fn asked_before(&self, issuer: &Issuer) -> ToolScopes {
        let before = self.before.as_ref().filter(|before| before.issuer == *issuer);
        before.map_or_default(|before| before.asks.clone())
    }

    /// The server there is that it's to, and the registration that was signed in to with before, where it was with one.
    pub(crate) fn registered(&self) -> Option<(&ToolServerId, &Registration)> {
        Some((self.to.there()?, self.before.as_ref()?.registration.as_ref()?))
    }
}
