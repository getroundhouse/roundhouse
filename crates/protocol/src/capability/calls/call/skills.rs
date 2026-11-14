//! Using one of the agent's skills.

use serde::{Deserialize, Serialize};

use crate::capability::prompts::SkillName;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UseSkill {
    pub name: SkillName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}
