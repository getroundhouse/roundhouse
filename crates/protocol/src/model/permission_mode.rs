//! How much an agent may do without asking.

use serde::{Deserialize, Serialize};

/// An agent's approval mode. Every agent has full access unless it's planning, so those are the only modes a chat can
/// be given. On the wire and in storage each is named as Claude's CLI names it, which is what logs and records held
/// before this was a type, so they read as they did.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PermissionMode {
    /// It never asks before using a tool.
    #[default]
    #[serde(rename = "bypassPermissions")]
    FullAccess,
    /// It works out a plan, and asks before changing anything until the plan is approved, which goes back to full
    /// access.
    #[serde(rename = "plan")]
    Plan,
    /// A mode this build has no name for: one an agent went into by itself, such as one that asks before every tool,
    /// or one from a newer peer. A chat can't be given it, and the hub starts an agent for a chat in it with full
    /// access.
    #[serde(other, rename = "unknown")]
    Unknown,
}

impl PermissionMode {
    /// Every mode a chat can be given.
    pub const CHOSEN: [PermissionMode; 2] = [PermissionMode::FullAccess, PermissionMode::Plan];

    /// Its name on the wire and in storage, which is its serde name, and what the command line takes. `None` for one
    /// this build has no name for.
    pub fn as_str(self) -> Option<&'static str> {
        match self {
            PermissionMode::FullAccess => Some("bypassPermissions"),
            PermissionMode::Plan => Some("plan"),
            PermissionMode::Unknown => None,
        }
    }

    /// The mode a chat can be given with this name, as [`PermissionMode::as_str`] gives it, or `None` for a name no
    /// such mode has.
    pub fn parse(name: &str) -> Option<PermissionMode> {
        PermissionMode::CHOSEN.into_iter().find(|mode| mode.as_str() == Some(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_a_chat_can_have_is_named_on_the_wire_as_claudes_cli_names_it() {
        let names = PermissionMode::CHOSEN.map(PermissionMode::as_str);
        assert_eq!(names, [Some("bypassPermissions"), Some("plan")]);

        for mode in PermissionMode::CHOSEN {
            let name = mode.as_str().unwrap();
            assert_eq!(serde_json::to_string(&mode).unwrap(), format!("\"{name}\""));
            assert_eq!(serde_json::from_str::<PermissionMode>(&format!("\"{name}\"")).unwrap(), mode);
            assert_eq!(PermissionMode::parse(name), Some(mode));
        }
    }

    #[test]
    fn any_other_mode_reads_as_unknown_and_cant_be_chosen() {
        for other in ["default", "acceptEdits", "manual", "unknown"] {
            let read = serde_json::from_str::<PermissionMode>(&format!("\"{other}\"")).unwrap();
            assert_eq!(read, PermissionMode::Unknown, "{other}");
            assert_eq!(PermissionMode::parse(other), None, "{other}");
        }
        assert_eq!(PermissionMode::Unknown.as_str(), None);
    }
}
