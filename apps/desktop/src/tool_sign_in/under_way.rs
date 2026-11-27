//! A sign-in there is, for whatever draws it: what it's to, and how it stands. Each stage holds what only it needs, so a
//! sign-in that ends, however it ends, lets go of its port, its page's address and what the person was sent back with.

use client_core::tool_server_edits::{Sent, Typed, TypedAuth};
use client_core::tool_servers::{SignIn, SignInStands};
use client_core::tool_sign_ins::NoPage;
use gpui_kit::{SharedString, Task};
use protocol::client::tool_sign_ins::{ToolSignInReturned, ToolSignInTo};
use protocol::model::{ToolServer, ToolServerId, ToolServerName, ToolServerUrl, ToolSignInId};

use super::page::SignInPage;
use crate::loopback::Bound;

/// The sign-in there is.
pub(crate) struct UnderWay {
    pub(super) id: ToolSignInId,
    pub(super) to: SignInTo,
    pub(super) stands: Stands,
}

impl UnderWay {
    pub(crate) fn to(&self) -> &SignInTo {
        &self.to
    }

    pub(crate) fn stands(&self) -> &Stands {
        &self.stands
    }

    /// As what's said of a server the hub has goes by it. `None` for one to a new server, which nothing is said of.
    pub(crate) fn of_a_server(&self) -> Option<SignIn> {
        let id = match &self.to {
            SignInTo::New(_) => return None,
            SignInTo::Again { id, .. } => id,
            SignInTo::Edited(edited) => &edited.was.id,
        };
        let stands = match &self.stands {
            Stands::Starting(_) | Stands::Waiting(_) | Stands::Finishing(_) => SignInStands::UnderWay,
            Stands::NoWay(no_page) => SignInStands::CameToNothing(no_page.why().to_string()),
            Stands::Failed(why) => SignInStands::CameToNothing(why.clone()),
        };
        Some(SignIn { to: id.clone(), stands })
    }
}

/// What a sign-in is to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignInTo {
    /// A new server, which the hub adds once it's approved.
    New(NewServer),
    /// One the hub has, which is signed in to again.
    Again { id: ToolServerId, name: ToolServerName, url: ToolServerUrl },
    /// One the hub has, as it's edited, which the hub saves once it's approved.
    Edited(EditedServer),
}

/// A server to add, as the person typed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewServer {
    pub(crate) id: ToolServerId,
    pub(crate) name: String,
    pub(crate) url: String,
}

/// A server the hub has, with its name and its URL as the person typed them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditedServer {
    /// As it was when they came to edit it.
    pub(crate) was: ToolServer,
    pub(crate) name: String,
    pub(crate) url: String,
}

impl SignInTo {
    /// The server the hub has, to sign in to it again.
    pub(crate) fn again(server: &ToolServer) -> SignInTo {
        SignInTo::Again { id: server.id.clone(), name: server.name.clone(), url: server.url.clone() }
    }

    /// What the person calls the server.
    pub(crate) fn name(&self) -> SharedString {
        match self {
            SignInTo::New(new) => new.name.trim().to_string().into(),
            SignInTo::Again { name, .. } => name.to_string().into(),
            SignInTo::Edited(edited) => edited.name.trim().to_string().into(),
        }
    }

    /// The host it's at, or all of what was typed where that isn't a URL, which the hub refuses.
    pub(crate) fn host(&self) -> String {
        let typed = match self {
            SignInTo::New(new) => &new.url,
            SignInTo::Again { url, .. } => return url.host().to_string(),
            SignInTo::Edited(edited) => &edited.url,
        };
        ToolServerUrl::typed(typed).map_or_else(|_| typed.clone(), |url| url.host().to_string())
    }

    /// As the hub is asked for it.
    pub(super) fn asked(&self) -> ToolSignInTo {
        match self {
            SignInTo::New(NewServer { id, name, url }) => {
                ToolSignInTo::New { id: id.clone(), name: name.clone(), url: url.clone() }
            }
            SignInTo::Again { id, .. } => ToolSignInTo::Again { id: id.clone() },
            SignInTo::Edited(EditedServer { was, name, url }) => {
                // Only what's changed, as saving the edit without a sign-in sends it.
                let typed = Typed { name: name.clone(), url: url.clone(), auth: TypedAuth::SignIn };
                let Sent { name, url, auth: _ } = typed.edit_of(was).into_sent();
                ToolSignInTo::Edit { id: was.id.clone(), name, url }
            }
        }
    }
}

/// How a sign-in stands.
pub(crate) enum Stands {
    /// The hub is asking the server how it's signed in to, which can take as long as 30 seconds.
    Starting(Starting),
    /// The browser has the page, and the person hasn't been sent back.
    Waiting(Waiting),
    /// They have, and the hub is finishing it.
    Finishing(Finishing),
    /// The server has no way to sign in, as this says, so nothing waits.
    NoWay(NoPage),
    /// It came to nothing, as this says, so nothing waits.
    Failed(String),
}

impl Stands {
    /// Whether the hub may have it waiting.
    pub(super) fn may_wait(&self) -> bool {
        match self {
            Stands::Starting(_) | Stands::Waiting(_) | Stands::Finishing(_) => true,
            Stands::NoWay(_) | Stands::Failed(_) => false,
        }
    }
}

/// What starting holds: the listener, which the hub has been told of, and which serves once there's a page.
pub(crate) struct Starting {
    pub(super) bound: Bound,
}

/// What waiting holds: the page, and the wait for the person to be sent back, which has the listener.
pub(crate) struct Waiting {
    pub(super) page: SignInPage,
    pub(super) _sent_back: Task<()>,
}

impl Waiting {
    /// The host of the page, where the person approves it.
    pub(crate) fn host(&self) -> &str {
        self.page.host()
    }
}

/// What finishing holds: what the person was sent back with, until the hub has answered it.
pub(crate) struct Finishing {
    pub(super) returned: ToolSignInReturned,
    /// The hub's round it was last sent in, as [`Hub::round`](client_core::Hub::round) says, and the wait for the
    /// answer.
    pub(super) sent: Option<(client_core::Round, Task<()>)>,
}
