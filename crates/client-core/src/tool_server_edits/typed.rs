use protocol::client::tool_servers::ToolServerAdd;
use protocol::model::{ToolServer, ToolServerAuth, ToolServerAuthGiven, ToolServerId, ToolServerKey};

use super::Edit;

/// A way a tool server can know the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    SignIn,
    Key,
    Neither,
}

impl Way {
    /// Every way, in the order a client offers them.
    pub const ALL: [Way; 3] = [Way::SignIn, Way::Key, Way::Neither];

    /// The one `auth` is, or `None` for one only a newer hub has.
    pub(super) fn of(auth: &ToolServerAuth) -> Option<Way> {
        match auth {
            ToolServerAuth::SignIn { .. } => Some(Way::SignIn),
            ToolServerAuth::Key { .. } => Some(Way::Key),
            ToolServerAuth::Neither => Some(Way::Neither),
            ToolServerAuth::Unknown => None,
        }
    }
}

/// A tool server as the person has typed it so far, to add it, or to save an edit of one the hub has. What's typed is
/// as it was typed, spaces and all, since the hub checks it.
#[derive(Clone, Debug)]
pub struct Typed {
    pub name: String,
    pub url: String,
    pub auth: TypedAuth,
}

/// How the server is to know the person, as it was chosen, with what's typed for that.
#[derive(Clone, Debug)]
pub enum TypedAuth {
    SignIn,
    /// By a key, sent in `header`. A key of nothing but spaces is none.
    Key {
        header: String,
        key: ToolServerKey,
    },
    Neither,
}

impl TypedAuth {
    pub fn way(&self) -> Way {
        match self {
            TypedAuth::SignIn => Way::SignIn,
            TypedAuth::Key { .. } => Way::Key,
            TypedAuth::Neither => Way::Neither,
        }
    }
}

/// What a form's main button does, as what's typed has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Does {
    Add,
    Save,
    /// Signs in, and adds or saves the server once that's approved.
    SignIn,
}

impl Typed {
    /// As an edit of `was`, the server the hub has.
    pub fn edit_of(&self, was: &ToolServer) -> Edit {
        Edit::of(was, self)
    }

    /// What the form's main button does, for a server to add, or for an edit of `was`, the one the hub has.
    pub fn does(&self, was: Option<&ToolServer>) -> Does {
        match was.map(|was| self.edit_of(was)) {
            None if self.auth.way() == Way::SignIn => Does::SignIn,
            None => Does::Add,
            Some(edit) if edit.signs_in() => Does::SignIn,
            Some(_) => Does::Save,
        }
    }

    /// Whether it has what adding it takes, or saving it as an edit of `was`: a name, a URL, and for a key, its header
    /// and a key, or the one the server has where that's kept.
    pub fn is_complete(&self, was: Option<&ToolServer>) -> bool {
        let auth = match &self.auth {
            TypedAuth::Key { header, key } => {
                filled(header) && (key_typed(key).is_some() || was.is_some_and(|was| self.edit_of(was).keeps_a_key()))
            }
            TypedAuth::SignIn | TypedAuth::Neither => true,
        };
        filled(&self.url) && filled(&self.name) && auth
    }

    /// What the hub is asked to add, as the server `id`. `None` for one to sign in to, which is the person's to approve
    /// first.
    pub fn added(&self, id: &ToolServerId) -> Option<ToolServerAdd> {
        let auth = match &self.auth {
            TypedAuth::SignIn => return None,
            TypedAuth::Key { header, key } => ToolServerAuthGiven::Key { header: header.clone(), key: key.clone() },
            TypedAuth::Neither => ToolServerAuthGiven::Neither,
        };
        Some(ToolServerAdd { id: id.clone(), name: self.name.clone(), url: self.url.clone(), auth })
    }
}

/// Whether anything but spaces is in `typed`.
fn filled(typed: &str) -> bool {
    !typed.trim().is_empty()
}

/// The key typed, unless none is.
pub(super) fn key_typed(key: &ToolServerKey) -> Option<&ToolServerKey> {
    filled(key.as_str()).then_some(key)
}
