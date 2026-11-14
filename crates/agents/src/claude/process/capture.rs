//! A later effort read also returns effective settings, which include the pinned login.
//! Captures keep only the applied model and effort, as the conformance fixtures do.

use serde_json::{Value, json};
use std::borrow::Cow;

pub(super) fn safe_line(line: &str) -> Cow<'_, str> {
    let Ok(mut value) = serde_json::from_str::<Value>(line) else { return Cow::Borrowed(line) };
    if value["type"] != "control_response" || value["response"]["response"].get("effective").is_none() {
        return Cow::Borrowed(line);
    }
    let applied = &value["response"]["response"]["applied"];
    value["response"]["response"] = json!({"applied":{"model":applied["model"], "effort":applied["effort"]}});
    Cow::Owned(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_capture_keeps_what_effort_checks_and_no_credentials() {
        let reply = json!({"type":"control_response", "response":{"subtype":"success", "request_id":"effort",
            "response":{"applied":{"model":"opus", "effort":"high"},
                "effective":{"env":{"CLAUDE_CODE_OAUTH_TOKEN":"sentinel-secret"}},
                "sources":{"user":{"apiKeyHelper":"sentinel-secret"}}}}});
        let line = reply.to_string();
        let captured = safe_line(&line);
        assert!(!captured.contains("sentinel-secret"));
        assert!(captured.contains("high"));
        assert!(captured.contains("effort"));
    }
}
