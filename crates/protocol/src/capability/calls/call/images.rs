//! Looking at an image, and making one.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ViewImage {
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GenerateImage {
    pub prompt: String,
    /// Images it starts from, by their paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub from: Vec<String>,
}
