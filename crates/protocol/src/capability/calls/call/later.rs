//! Waiting, and setting a prompt to send later.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sleep {
    pub duration_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub prompt: String,
    pub when: When,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "when", rename_all = "snake_case")]
pub enum When {
    After {
        seconds: u64,
    },
    /// A cron expression, once or every time it matches.
    Cron {
        expression: String,
        repeats: bool,
    },
    #[serde(other)]
    Unknown,
}
