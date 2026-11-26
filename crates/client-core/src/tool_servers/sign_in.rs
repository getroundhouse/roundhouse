use protocol::model::ToolServerId;

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

impl SignIn {
    /// How it stands, if it's to `server`.
    pub(super) fn stands_of(&self, server: &ToolServerId) -> Option<&SignInStands> {
        (self.to == *server).then_some(&self.stands)
    }
}
