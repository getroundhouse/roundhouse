//! How hard an agent thinks before it answers.

use serde::{Deserialize, Serialize};

/// A level, from least to most. What a level means for each model, and what an agent calls it, is the agent
/// integration's to decide.
///
/// It reads from any string, so a level this build can't take, such as one whose name isn't an [`EffortId`], reads as
/// `Unknown` rather than failing the frame or the row it's in.
///
/// [`EffortId`]: crate::capability::settings::EffortId
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase", from = "String")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
    None,
    Minimal,
    Persistent,
    /// The stored setting for a model with no effort control. The empty string cannot be an adapter ID.
    #[serde(rename = "")]
    Unset,
    Unknown,
    #[serde(untagged)]
    Named(crate::capability::settings::EffortId),
}

impl Effort {
    /// The original six picker levels, used by static catalogs.
    pub const ALL: [Effort; 6] = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max, Effort::Ultra];

    /// A setting that can be passed to an adapter, including an explicit clear.
    pub fn known(&self) -> Option<Self> {
        (self != &Self::Unknown).then(|| self.clone())
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            Effort::Low => Some("Low"),
            Effort::Medium => Some("Medium"),
            Effort::High => Some("High"),
            Effort::Xhigh => Some("Extra high"),
            Effort::Max => Some("Max"),
            Effort::Ultra => Some("Ultra"),
            Effort::None | Effort::Minimal | Effort::Persistent | Effort::Named(_) => self.as_str(),
            Effort::Unknown | Effort::Unset => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        Some(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Persistent => "persistent",
            Self::Named(id) => id.as_str(),
            Self::Unknown | Self::Unset => return None,
        })
    }

    /// The level with this name, as [`Effort::as_str`] gives it, or `None` for a name no level can have.
    pub fn parse(name: &str) -> Option<Self> {
        let effort = Self::from(name.to_owned());
        effort.as_str()?;
        Some(effort)
    }

    /// The level this build names itself, of those it has a variant for.
    pub(crate) fn fixed(name: &str) -> Option<Self> {
        Some(match name {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" => Self::Xhigh,
            "max" => Self::Max,
            "ultra" => Self::Ultra,
            "none" => Self::None,
            "minimal" => Self::Minimal,
            "persistent" => Self::Persistent,
            "" => Self::Unset,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

impl From<String> for Effort {
    fn from(name: String) -> Self {
        Self::fixed(&name)
            .unwrap_or_else(|| crate::capability::settings::EffortId::try_from(name).map_or(Self::Unknown, Self::Named))
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
    fn a_new_effort_keeps_its_wire_name() {
        let newer = serde_json::from_str::<Effort>("\"future\"").unwrap();

        assert_eq!(newer.as_str(), Some("future"));
        assert_eq!(serde_json::to_string(&newer).unwrap(), r#""future""#);
        assert_eq!(Effort::parse("future"), Some(newer));
        assert_eq!(Effort::parse("unknown"), None);
    }

    /// Rather than failing the frame or the row it's in, such as a session's.
    #[test]
    fn a_name_no_level_can_have_reads_as_unknown() {
        for name in ["\"with space\"", "\"café\"", "\"unknown\""] {
            assert_eq!(serde_json::from_str::<Effort>(name).unwrap(), Effort::Unknown, "{name}");
        }
        assert_eq!(serde_json::from_str::<Effort>("\"\"").unwrap(), Effort::Unset);
        assert_eq!(Effort::parse("with space"), None);
    }

    #[test]
    fn every_other_level_is_known() {
        for effort in Effort::ALL {
            assert_eq!(effort.known(), Some(effort));
        }
    }
}
