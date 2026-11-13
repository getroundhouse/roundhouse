//! Which settings a running agent takes a change of, and when.

use serde::{Deserialize, Serialize};

/// The settings a running agent takes a change of, and when it takes each, which a client says as the person makes
/// it. One that's `None` waits for the next launch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct LiveChanges {
    pub model: Option<Takes>,
    pub mode: Option<Takes>,
    pub effort: Option<Takes>,
    pub speed: Option<Takes>,
    pub thinking: Option<Takes>,
    pub persona: Option<Takes>,
    pub style: Option<Takes>,
    pub verbosity: Option<Takes>,
    pub advisor: Option<Takes>,
    pub budget: Option<Takes>,
}

/// When a running agent takes a change it agreed to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Takes {
    /// At once, mid-turn included, as Claude takes a new approval mode.
    Now,
    /// At its next turn, as Claude takes a new model, and Codex takes what a turn starts with.
    NextTurn,
    #[serde(other)]
    Unknown,
}
