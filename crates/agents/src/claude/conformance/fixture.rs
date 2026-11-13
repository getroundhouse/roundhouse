//! A scripted trial's capture as a fixture, with what the CLI and the adapter make afresh every trial made the same
//! each time, so a fixture captured again differs only where one of them did something else, which includes where and
//! how often the adapter asks by itself, and what the CLI answers it.

use std::path::PathBuf;

use serde_json::Value;

use super::stream_json::StreamJson;
use crate::conformance::capture::{self, Entry};

/// What the CLI says of time: how long each part of a turn took, and when it happened.
const TIMES: &[&str] = &[
    "duration_ms",
    "duration_api_ms",
    "thinking_duration_ms",
    "first_content_frame_ms",
    "request_sent_wall_ms",
    "time_to_request_ms",
    "ttft_ms",
    "ttft_stream_ms",
    "total_api_duration_ms",
    "total_duration_ms",
    "timestamp",
];

/// When a fixture says everything happened.
const WHEN: &str = "2000-01-01T00:00:00.000Z";

/// `captured` as a fixture, with each of `named`'s strings replaced by the name it's given, as for
/// [`capture::fixture`], its times zeroed, the CLI's and any the adapter echoes back to it alike, and each plan file in
/// one of `plans`, where the CLI keeps them, named by the order the CLI first names them, since it makes up their names.
pub(super) fn fixture(captured: &[Entry], mut named: Vec<(String, String)>, plans: &[PathBuf]) -> String {
    let text = serde_json::to_string(captured).unwrap_or_default();
    let mut made_up: Vec<String> = Vec::new();
    for file in plans.iter().flat_map(|dir| plan_files(&text, &format!("{}/", dir.display()))) {
        if !made_up.contains(&file) {
            made_up.push(file);
        }
    }
    named.extend(made_up.into_iter().enumerate().map(|(number, file)| (file, format!("plan-{}.md", number + 1))));
    let untimed: Vec<Entry> = captured
        .iter()
        .cloned()
        .map(|mut entry| {
            if let Entry::FromCli(line) | Entry::ToCli(line) = &mut entry {
                untime(line);
            }
            entry
        })
        .collect();
    capture::fixture(&untimed, &named, &StreamJson)
}

/// The name of each plan file `text` names in `plans`, a directory, with the `/` it ends with, in the order it first
/// names them.
pub(super) fn plan_files(text: &str, plans: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (at, _) in text.match_indices(plans) {
        let name = text[at + plans.len()..].split(['"', '\\', ' ']).next().unwrap_or_default();
        if !name.is_empty() && !name.contains('/') && !names.iter().any(|named| named == name) {
            names.push(name.to_string());
        }
    }
    names
}

fn untime(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if !TIMES.contains(&key.as_str()) {
                    untime(value);
                } else if value.is_number() {
                    *value = Value::from(0);
                } else if value.is_string() {
                    *value = Value::from(WHEN);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(untime),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::fixture;
    use crate::conformance::capture::Entry;

    /// A turn whose adapter asks for usage and the context's as it ends, in `order`, and asks for the context's again
    /// `again` times.
    fn turn(order: [&str; 2], again: usize) -> Vec<Entry> {
        let (prompt, said) = ("dc55a3a8-9bd0-48f6-baa4-4bac05623980", "048bb47c-5317-495c-988d-a0a8819ba0d1");
        let ask = |kind: &str, id: usize| {
            let id = format!("37d5a0a3-48f4-4b53-aeda-{id:012x}");
            Entry::ToCli(json!({"type": "control_request", "request_id": id, "request": {"subtype": kind}}))
        };
        let mut entries = vec![
            Entry::ToCli(json!({"type": "user", "uuid": prompt})),
            Entry::FromCli(json!({"type": "assistant", "uuid": said, "command_uuid": prompt})),
        ];
        entries.extend(order.iter().enumerate().map(|(id, kind)| ask(kind, id + 1)));
        entries.extend((0..again).map(|id| ask("get_context_usage", id + 10)));
        entries.push(Entry::FromCli(json!({"type": "result", "uuid": said})));
        entries
    }

    #[test]
    fn where_and_how_often_the_adapter_asks_by_itself_renumbers_nothing() {
        let numbered = |entries: &[Entry]| -> Vec<Value> {
            fixture(entries, Vec::new(), &[]).lines().map(|line| serde_json::from_str(line).unwrap()).collect()
        };
        let one = numbered(&turn(["get_usage", "get_context_usage"], 0));
        let other = numbered(&turn(["get_context_usage", "get_usage"], 1));

        let ask = |lines: &[Value], kind: &str| {
            let asked = lines.iter().find(|line| line["to_cli"]["request"]["subtype"] == kind).unwrap();
            asked["to_cli"]["request_id"].clone()
        };
        assert_eq!(ask(&one, "get_usage"), ask(&other, "get_usage"));
        assert_eq!(ask(&one, "get_context_usage"), ask(&other, "get_context_usage"));
        assert_ne!(ask(&one, "get_usage"), ask(&one, "get_context_usage"));
        let rest = |lines: &[Value]| -> Vec<Value> {
            lines.iter().filter(|line| line["to_cli"]["type"] != "control_request").cloned().collect()
        };
        assert_eq!(rest(&one), rest(&other));
        assert_eq!(one[0]["to_cli"]["uuid"], "00000000-0000-4000-8000-000000000001");
    }

    #[test]
    fn a_fixture_says_nothing_of_when_and_names_each_plan_by_its_order() {
        let plans = "/tmp/claude-conformance-x/home/.claude/plans/";
        let entries = [
            Entry::FromCli(json!({"type": "result", "duration_ms": 90, "result": "done", "num_turns": 1})),
            Entry::FromCli(json!({"type": "user", "timestamp": "2026-11-13T20:18:15.034Z"})),
            Entry::FromCli(json!({"input": {"planFilePath": format!("{plans}write-it-jiggly-otter.md")}})),
            Entry::ToCli(json!({"updatedInput": {"planFilePath": format!("{plans}write-it-jiggly-otter.md")}})),
        ];

        let named = vec![("/tmp/claude-conformance-x".to_string(), "/scratch".to_string())];
        let text = fixture(&entries, named, &[PathBuf::from(plans.trim_end_matches('/'))]);

        let lines: Vec<Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(lines[0]["from_cli"], json!({"type": "result", "duration_ms": 0, "result": "done", "num_turns": 1}));
        assert_eq!(lines[1]["from_cli"]["timestamp"], "2000-01-01T00:00:00.000Z");
        let plan = "/scratch/home/.claude/plans/plan-1.md";
        assert_eq!(lines[2]["from_cli"]["input"]["planFilePath"], plan);
        assert_eq!(lines[3]["to_cli"]["updatedInput"]["planFilePath"], plan);
    }
}
