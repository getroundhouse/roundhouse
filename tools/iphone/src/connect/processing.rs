//! Waiting for App Store Connect to process an upload, which can reject it after `xcodebuild` said it succeeded.

use std::time::{Duration, Instant};

use anyhow::bail;
use serde_json::Value;

use super::Client;
use super::record::AppRecord;
use crate::stamp::BuildNumber;

/// How often to ask, since processing takes minutes.
const ASK_EVERY: Duration = Duration::from_secs(30);

/// A build App Store Connect has processed, by its ID there.
pub(crate) struct BuildId(pub(super) String);

/// Where App Store Connect is with an upload.
#[derive(Debug, PartialEq, Eq)]
enum Processing {
    /// Not listed yet, as it isn't for a minute or so after the upload.
    NotListed,
    Underway,
    Processed(String),
    /// Rejected, as `FAILED` or `INVALID`, which Apple's email to the team says why.
    Rejected(String),
}

/// Waits until App Store Connect has processed `build`, and fails if it rejects it or takes longer than `within`.
pub(crate) fn wait_for_processing(
    client: &Client<'_>,
    app: &AppRecord,
    build: &BuildNumber,
    within: Duration,
) -> anyhow::Result<BuildId> {
    let path = format!("/v1/builds?filter%5Bapp%5D={}&filter%5Bversion%5D={build}&limit=1", app.id);
    let deadline = Instant::now() + within;
    loop {
        match state(&client.get(&path)?) {
            Processing::Processed(id) => return Ok(BuildId(id)),
            Processing::Rejected(state) => {
                bail!("App Store Connect rejected build {build} as {state}. Apple's email to the team says why")
            }
            Processing::NotListed | Processing::Underway if Instant::now() + ASK_EVERY > deadline => {
                bail!("App Store Connect didn't process build {build} within {} minutes", within.as_secs() / 60)
            }
            Processing::NotListed => {
                println!("Build {build} isn't listed yet");
                pause();
            }
            Processing::Underway => {
                println!("Build {build} is being processed");
                pause();
            }
        }
    }
}

#[expect(clippy::disallowed_methods, reason = "the pace between looks at App Store Connect, which takes minutes")]
fn pause() {
    std::thread::sleep(ASK_EVERY);
}

/// Where the build in App Store Connect's answer to a request for it is.
fn state(builds: &Value) -> Processing {
    let Some(build) = builds["data"].as_array().and_then(|builds| builds.first()) else { return Processing::NotListed };
    match build["attributes"]["processingState"].as_str() {
        Some("VALID") => Processing::Processed(build["id"].as_str().unwrap_or_default().to_string()),
        Some(rejected @ ("FAILED" | "INVALID")) => Processing::Rejected(rejected.to_string()),
        _ => Processing::Underway,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_build_is_processed_when_valid_rejected_when_failed_or_invalid_and_otherwise_still_going() {
        let build = |state: &str| json!({ "data": [{ "id": "b1", "attributes": { "processingState": state } }] });
        let cases = [
            (json!({ "data": [] }), Processing::NotListed),
            (build("PROCESSING"), Processing::Underway),
            (build("VALID"), Processing::Processed("b1".into())),
            (build("FAILED"), Processing::Rejected("FAILED".into())),
            (build("INVALID"), Processing::Rejected("INVALID".into())),
        ];

        for (answer, expected) in cases {
            assert_eq!(state(&answer), expected, "{answer}");
        }
    }
}
