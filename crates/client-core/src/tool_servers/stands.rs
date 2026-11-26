use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::model::{ToolServer, ToolServerId};

use super::{SignIn, SignInFor, SignInStands};
use crate::calls::count;

/// How one of a chat's tool servers stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stands {
    /// Nobody has said, as of every server of an agent that says nothing of its servers.
    NotSaid,
    Tools(u32),
    /// With how many tools not known.
    Connected,
    Starting,
    /// One of the hub's, `server`, which the person signs in to from a client, and for what, with how this client's
    /// sign-in to it stands, if it has one.
    NeedsSignIn {
        server: ToolServerId,
        wanted: SignInFor,
        sign_in: Option<SignInStands>,
    },
    /// One of the CLI's own, which no client can sign in to.
    SignInFromItsCli,
    /// With why, on one line, where the agent says.
    CantBeReached {
        why: Option<String>,
    },
    /// By the CLI's settings.
    TurnedOff,
    /// One of the hub's that the agent wasn't launched with, or one that's been moved to another URL since the agent
    /// connected to it, so its worker has cut the agent off from it.
    FromItsNextStart,
    /// One the agent has that the hub no longer does.
    Removed,
}

/// How strongly what's said of a server is said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    /// What's coming, without anyone doing anything.
    Coming,
    /// Something's in the way.
    Problem,
}

impl Stands {
    /// One the CLI has of its own, as the agent says it.
    pub(super) fn of_its_own(said: &ToolServerState) -> Self {
        match said.state {
            ServerState::NeedsSignIn => Stands::SignInFromItsCli,
            ServerState::Starting
            | ServerState::Connected
            | ServerState::Failed
            | ServerState::Disabled
            | ServerState::Unknown => Self::said(said),
        }
    }

    /// The hub's `server`, as its record has it, and as the agent says it, where the agent was launched with it.
    /// `agents_said` is whether the agent has said which it has, and `sign_in` the one this client has.
    pub(super) fn of_the_hubs(
        server: &ToolServer,
        said: Option<&ToolServerState>,
        agents_said: bool,
        sign_in: Option<&SignIn>,
    ) -> Self {
        // As the hub knows of its own sign-ins, whatever the agent makes of what its worker answered. The same goes for
        // one that works and wants more access, since what there is to do comes before how many tools it has.
        if let Some(wanted) = SignInFor::of(&server.auth) {
            let sign_in = sign_in.and_then(|sign_in| sign_in.stands_of(&server.id)).cloned();
            return Stands::NeedsSignIn { server: server.id.clone(), wanted, sign_in };
        }
        match said {
            Some(said) if said.connected_to.as_ref().is_some_and(|place| *place != server.url.place()) => {
                Stands::FromItsNextStart
            }
            Some(said) => Self::said(said),
            None if agents_said => Stands::FromItsNextStart,
            None => Stands::NotSaid,
        }
    }

    fn said(said: &ToolServerState) -> Self {
        match said.state {
            ServerState::Connected => said.tools.map_or(Stands::Connected, Stands::Tools),
            ServerState::Starting => Stands::Starting,
            ServerState::Failed => Stands::CantBeReached { why: said.error.clone() },
            // One of the hub's that isn't a sign-in's, which the agent can't use whatever it makes of why.
            ServerState::NeedsSignIn => Stands::CantBeReached { why: None },
            ServerState::Disabled => Stands::TurnedOff,
            ServerState::Unknown => Stands::NotSaid,
        }
    }

    /// What's said of it, if anything is.
    pub fn words(&self) -> Option<String> {
        let words = match self {
            Stands::NotSaid => return None,
            Stands::Tools(tools) => return Some(count(usize::try_from(*tools).unwrap_or(usize::MAX), "tool", "tools")),
            Stands::Connected => "Connected",
            Stands::Starting => "Starting",
            Stands::NeedsSignIn { sign_in: Some(SignInStands::UnderWay), .. } => "Signing in",
            Stands::NeedsSignIn { wanted, sign_in: None | Some(SignInStands::CameToNothing(_)), .. } => match wanted {
                SignInFor::Use => "Needs you to sign in",
                SignInFor::MoreAccess => "Needs you to sign in again",
            },
            Stands::SignInFromItsCli => "Sign in from its CLI",
            Stands::CantBeReached { .. } => "Can't be reached",
            Stands::TurnedOff => "Turned off",
            Stands::FromItsNextStart => "From its next start",
            Stands::Removed => "Removed",
        };
        Some(words.to_string())
    }

    pub fn tone(&self) -> Tone {
        match self {
            Stands::NotSaid
            | Stands::Tools(_)
            | Stands::Connected
            | Stands::Starting
            | Stands::TurnedOff
            | Stands::Removed => Tone::Plain,
            Stands::FromItsNextStart | Stands::NeedsSignIn { sign_in: Some(SignInStands::UnderWay), .. } => {
                Tone::Coming
            }
            Stands::NeedsSignIn { sign_in: None | Some(SignInStands::CameToNothing(_)), .. }
            | Stands::SignInFromItsCli
            | Stands::CantBeReached { .. } => Tone::Problem,
        }
    }

    /// The server a client offers to sign in to: one of the hub's that needs it, with no sign-in under way.
    pub fn to_sign_in_to(&self) -> Option<&ToolServerId> {
        match self {
            Stands::NeedsSignIn { server, sign_in: None | Some(SignInStands::CameToNothing(_)), .. } => Some(server),
            Stands::NeedsSignIn { sign_in: Some(SignInStands::UnderWay), .. }
            | Stands::NotSaid
            | Stands::Tools(_)
            | Stands::Connected
            | Stands::Starting
            | Stands::SignInFromItsCli
            | Stands::CantBeReached { .. }
            | Stands::TurnedOff
            | Stands::FromItsNextStart
            | Stands::Removed => None,
        }
    }

    /// What the person has to sign in to it for, as a server of the hub's, whether or not they're doing so.
    pub fn sign_in_for(&self) -> Option<SignInFor> {
        match self {
            Stands::NeedsSignIn { wanted, .. } => Some(*wanted),
            Stands::NotSaid
            | Stands::Tools(_)
            | Stands::Connected
            | Stands::Starting
            | Stands::SignInFromItsCli
            | Stands::CantBeReached { .. }
            | Stands::TurnedOff
            | Stands::FromItsNextStart
            | Stands::Removed => None,
        }
    }

    /// On one line: why it can't be reached, where the agent says, why this client's sign-in to it came to nothing, or
    /// that it's signed in to again for more access.
    pub fn why(&self) -> Option<&str> {
        match self {
            Stands::CantBeReached { why } => why.as_deref(),
            Stands::NeedsSignIn { sign_in: Some(SignInStands::CameToNothing(why)), .. } => Some(why),
            Stands::NeedsSignIn { wanted: SignInFor::MoreAccess, sign_in: None, .. } => Some("Wants more access"),
            Stands::NeedsSignIn { wanted: SignInFor::Use, sign_in: None, .. }
            | Stands::NeedsSignIn { sign_in: Some(SignInStands::UnderWay), .. }
            | Stands::NotSaid
            | Stands::Tools(_)
            | Stands::Connected
            | Stands::Starting
            | Stands::SignInFromItsCli
            | Stands::TurnedOff
            | Stands::FromItsNextStart
            | Stands::Removed => None,
        }
    }
}
