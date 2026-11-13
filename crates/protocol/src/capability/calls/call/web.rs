//! Reading the web.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Fetch {
    pub url: String,
    /// What the agent asked of the page, when the tool answers that rather than sending the page, as Claude's does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asking: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WebSearch {
    pub query: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_domains: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FindInPage {
    pub url: String,
    pub pattern: String,
}
