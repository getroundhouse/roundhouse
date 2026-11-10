//! How hard an agent thinks before it answers.

use serde::{Deserialize, Serialize};

/// A level, from least to most. What a level means for each model, and what an agent calls it, is the agent
/// integration's to decide.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    #[serde(other)]
    Unknown,
}

impl Effort {
    /// Every level, from least to most.
    pub const ALL: [Effort; 5] = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max];

    /// The level, or `None` for one from a newer peer, which can't be passed on, since this build can't name it.
    pub fn known(self) -> Option<Effort> {
        match self {
            Effort::Low | Effort::Medium | Effort::High | Effort::Xhigh | Effort::Max => Some(self),
            Effort::Unknown => None,
        }
    }

    /// Its name on the wire and in storage, which is its serde name, and what the command line takes. `None` for one
    /// from a newer peer.
    pub fn as_str(self) -> Option<&'static str> {
        match self {
            Effort::Low => Some("low"),
            Effort::Medium => Some("medium"),
            Effort::High => Some("high"),
            Effort::Xhigh => Some("xhigh"),
            Effort::Max => Some("max"),
            Effort::Unknown => None,
        }
    }

    /// The level with this name, as [`Effort::as_str`] gives it, or `None` for a name no level has.
    pub fn parse(name: &str) -> Option<Effort> {
        Effort::ALL.into_iter().find(|effort| effort.as_str() == Some(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_is_named_on_the_wire_as_it_names_itself() {
        for effort in Effort::ALL {
            let name = effort.as_str().unwrap();

            assert_eq!(serde_json::to_string(&effort).unwrap(), format!("\"{name}\""));
            assert_eq!(serde_json::from_str::<Effort>(&format!("\"{name}\"")).unwrap(), effort);
            assert_eq!(Effort::parse(name), Some(effort));
        }
    }

    #[test]
    fn a_level_from_a_newer_peer_reads_as_unknown_and_isnt_known() {
        let newer = serde_json::from_str::<Effort>("\"ultra\"").unwrap();

        assert_eq!(newer, Effort::Unknown);
        assert_eq!(newer.known(), None);
        assert_eq!(Effort::parse("ultra"), None);
        assert_eq!(Effort::parse("unknown"), None);
    }

    #[test]
    fn every_other_level_is_known() {
        for effort in Effort::ALL {
            assert_eq!(effort.known(), Some(effort));
        }
    }
}
