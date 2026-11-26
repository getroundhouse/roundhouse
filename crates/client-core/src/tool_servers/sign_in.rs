use protocol::model::{ToolServerAuth, ToolServerId, ToolSignInStatus};

/// A sign-in to one of the hub's tool servers, from this client, as far as what's said of the server goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignIn {
    pub to: ToolServerId,
    pub stands: SignInStands,
}

/// How a sign-in stands. There's none once it has signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInStands {
    UnderWay,
    /// It came to nothing, as this says.
    CameToNothing(String),
}

/// What the person has to sign in to one of the hub's tool servers for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInFor {
    /// For any agent to use it at all, since its sign-in is gone.
    Use,
    /// For a call it refused. It works for the rest.
    MoreAccess,
}

impl SignInFor {
    /// What a server that knows the person by `auth` needs them to sign in for, if it does.
    pub(super) fn of(auth: &ToolServerAuth) -> Option<Self> {
        match auth {
            ToolServerAuth::SignIn { status } => match status {
                ToolSignInStatus::NeedsSignIn => Some(SignInFor::Use),
                ToolSignInStatus::NeedsMoreAccess => Some(SignInFor::MoreAccess),
                ToolSignInStatus::Works | ToolSignInStatus::Unknown => None,
            },
            ToolServerAuth::Key { .. } | ToolServerAuth::Neither | ToolServerAuth::Unknown => None,
        }
    }
}

impl SignIn {
    /// How it stands, if it's to `server`.
    pub(super) fn stands_of(&self, server: &ToolServerId) -> Option<&SignInStands> {
        (self.to == *server).then_some(&self.stands)
    }
}
