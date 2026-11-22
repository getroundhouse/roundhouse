//! What a build tells its testers to try, which `TestFlight` shows them as "What to Test".

use serde_json::{Value, json};

use super::Client;
use super::client::Method;
use super::processing::BuildId;
use super::record::AppRecord;

/// The longest notes App Store Connect takes.
const LONGEST: usize = 4000;
const CUT: &str = "\n…and more, which the repo's history has.";

/// Notes cut short at App Store Connect's limit, at a line's end, with a line that says so.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Notes(String);

impl Notes {
    pub(crate) fn new(text: &str) -> Self {
        let text = text.trim();
        if text.chars().count() <= LONGEST {
            return Self(text.to_string());
        }
        let room = LONGEST - CUT.chars().count();
        let kept: String = text.chars().take(room).collect();
        let kept = kept.rfind('\n').map_or(kept.as_str(), |end| &kept[..end]);
        Self(format!("{kept}{CUT}"))
    }
}

/// Gives the build `notes` in each language it has notes in, or in the app's own if it has none yet.
pub(crate) fn set_notes(client: &Client<'_>, app: &AppRecord, build: &BuildId, notes: &Notes) -> anyhow::Result<()> {
    let existing = client.get(&format!("/v1/builds/{}/betaBuildLocalizations", build.0))?;
    for (method, path, body) in requests(&existing, app, build, notes) {
        client.send(method, &path, &body)?;
    }
    Ok(())
}

fn requests(existing: &Value, app: &AppRecord, build: &BuildId, notes: &Notes) -> Vec<(Method, String, Value)> {
    let ids: Vec<&str> = existing["data"].as_array().into_iter().flatten().filter_map(|l| l["id"].as_str()).collect();
    if ids.is_empty() {
        let body = json!({ "data": {
            "type": "betaBuildLocalizations",
            "attributes": { "locale": app.primary_locale, "whatsNew": notes.0 },
            "relationships": { "build": { "data": { "type": "builds", "id": build.0 } } },
        } });
        return vec![(Method::Post, "/v1/betaBuildLocalizations".to_string(), body)];
    }
    let patch = |id: &str| {
        let body =
            json!({ "data": { "type": "betaBuildLocalizations", "id": id, "attributes": { "whatsNew": notes.0 } } });
        (Method::Patch, format!("/v1/betaBuildLocalizations/{id}"), body)
    };
    ids.into_iter().map(patch).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_past_the_limit_are_cut_at_a_lines_end_and_say_so() {
        let line = "- A title of a commit that changed the app (#1234)\n";
        let long = line.repeat(200);

        let notes = Notes::new(&long);

        assert!(notes.0.chars().count() <= LONGEST);
        assert!(notes.0.ends_with(&format!("(#1234){CUT}")));
        assert_eq!(Notes::new("  Built from abc.\n"), Notes("Built from abc.".to_string()));
    }

    #[test]
    fn a_build_with_notes_has_each_language_changed_and_one_without_gets_the_apps_language() {
        let app = AppRecord { id: "app".into(), primary_locale: "en-US".into() };
        let (build, notes) = (BuildId("b1".into()), Notes::new("Try it"));
        let two = json!({ "data": [{ "id": "l1" }, { "id": "l2" }] });

        let changed = requests(&two, &app, &build, &notes);
        let made = requests(&json!({ "data": [] }), &app, &build, &notes);

        let said = |requests: &[(Method, String, Value)]| -> Vec<(Method, String)> {
            requests.iter().map(|(method, path, _)| (*method, path.clone())).collect()
        };
        assert_eq!(
            said(&changed),
            [
                (Method::Patch, "/v1/betaBuildLocalizations/l1".to_string()),
                (Method::Patch, "/v1/betaBuildLocalizations/l2".to_string())
            ]
        );
        assert_eq!(changed[0].2["data"]["attributes"], json!({ "whatsNew": "Try it" }));
        assert_eq!(said(&made), [(Method::Post, "/v1/betaBuildLocalizations".to_string())]);
        assert_eq!(made[0].2["data"]["attributes"], json!({ "locale": "en-US", "whatsNew": "Try it" }));
        assert_eq!(made[0].2["data"]["relationships"]["build"]["data"]["id"], "b1");
    }
}
