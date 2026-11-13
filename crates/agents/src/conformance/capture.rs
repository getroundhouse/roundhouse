//! A capture: each launch of an agent's CLI in a trial, each line its adapter wrote to it and read from it, in the
//! order the adapter did, and how each exited. A fixture keeps a scripted trial's, and a replay plays one back.

use std::collections::{BTreeMap, HashMap};

use protocol::model::AgentSessionId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use super::lines::{Lines, asked_by_itself};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    /// The adapter started the CLI with `args`, as a new conversation or resuming `session`.
    Launch { resume: bool, session: AgentSessionId, args: Vec<String> },
    /// A line the adapter wrote to the CLI.
    ToCli(Value),
    /// A line the CLI wrote, which the adapter read.
    FromCli(Value),
    /// A line the CLI wrote that isn't JSON.
    FromCliText(String),
    /// A step saw what only the CLI's world shows, such as a file a command made, once the adapter had read what's
    /// before this.
    Saw(String),
    /// The process exited, as the adapter heard.
    Exit { code: Option<i32>, signal: Option<i32>, requested: bool },
}

/// What the adapter tells each launch, exit and line, which it sends on as an [`Entry`], in order.
#[derive(Clone, Debug)]
pub struct Tap(mpsc::UnboundedSender<Entry>);

impl Tap {
    /// A tap, and where what it's told arrives.
    pub fn new() -> (Tap, mpsc::UnboundedReceiver<Entry>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Tap(sender), receiver)
    }

    pub(crate) fn note(&self, entry: Entry) {
        // Nothing listens once the trial that made it has finished, and then there's nothing to capture.
        let _ = self.0.send(entry);
    }

    pub(crate) fn launched(&self, resume: bool, session: &AgentSessionId, args: &[String]) {
        self.note(Entry::Launch { resume, session: session.clone(), args: args.to_vec() });
    }

    pub(crate) fn wrote(&self, line: &Value) {
        self.note(Entry::ToCli(line.clone()));
    }

    pub(crate) fn read(&self, line: &str) {
        self.note(serde_json::from_str(line).map_or_else(|_| Entry::FromCliText(line.to_string()), Entry::FromCli));
    }

    pub(crate) fn exited(&self, code: Option<i32>, signal: Option<i32>, requested: bool) {
        self.note(Entry::Exit { code, signal, requested });
    }
}

/// A fixture's text: one entry a line, with each of `named`'s strings replaced by the name it's given, such as a
/// directory that differs every trial, and each UUID by one numbered in the order it first appears, since the CLI and
/// the adapter make them afresh every trial. The IDs of each kind of request the adapter makes by itself, as `lines`
/// say, are numbered in a range of their own, since where it makes them and how often varies, so how they fall
/// renumbers nothing else. A range is the fourth group of a version 4 UUID, from `8000`. Both look in every string and
/// key, at what the string holds rather than how JSON writes it.
pub fn fixture(entries: &[Entry], named: &[(String, String)], lines: &dyn Lines) -> String {
    let mut kinds: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (kind, id) in asked_by_itself(lines, entries) {
        kinds.entry(kind).or_default().push(id);
    }
    let apart: Vec<Vec<&str>> = kinds.into_values().collect();
    // An entry is a value this module made, so it always serializes.
    let mut values: Vec<Value> = entries.iter().map(|entry| serde_json::to_value(entry).unwrap_or_default()).collect();
    let mut found: Vec<String> = Vec::new();
    for value in &mut values {
        each_string(value, &mut |text| {
            for (from, name) in named {
                *text = text.replace(from.as_str(), name);
            }
            found.extend(uuids(text).map(|(_, id)| id.to_string()));
        });
    }
    let mut numbered: HashMap<&str, String> = HashMap::new();
    let first = found.iter().map(String::as_str).filter(|id| !apart.iter().flatten().any(|set| set == id));
    let sets = std::iter::once(first.collect::<Vec<_>>()).chain(apart.iter().cloned());
    for (range, ids) in sets.enumerate() {
        let mut last = 0;
        for id in ids {
            numbered.entry(id).or_insert_with(|| {
                last += 1;
                format!("00000000-0000-4000-{:04x}-{last:012}", 0x8000 + range)
            });
        }
    }
    let mut text = String::new();
    for mut value in values {
        each_string(&mut value, &mut |string| {
            let mut renumbered = String::new();
            let mut written = 0;
            for (at, id) in uuids(string) {
                renumbered.push_str(&string[written..at]);
                // The first walk numbered every UUID this one finds.
                renumbered.push_str(&numbered[id]);
                written = at + id.len();
            }
            renumbered.push_str(&string[written..]);
            *string = renumbered;
        });
        text.push_str(&value.to_string());
        text.push('\n');
    }
    text
}

/// Changes each string in `value`, and each key.
fn each_string(value: &mut Value, change: &mut impl FnMut(&mut String)) {
    match value {
        Value::String(text) => change(text),
        Value::Array(items) => items.iter_mut().for_each(|item| each_string(item, change)),
        Value::Object(map) => {
            for (mut key, mut item) in std::mem::take(map) {
                change(&mut key);
                each_string(&mut item, change);
                map.insert(key, item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Where each UUID in `text` is, in order, and what it is, as the CLI and the adapter write them: lowercase, with
/// hyphens, and apart from the letters and digits around it.
fn uuids(text: &str) -> impl Iterator<Item = (usize, &str)> {
    const LENGTH: usize = 36;
    let bytes = text.as_bytes();
    let is_uuid = |at: usize| {
        let candidate = &bytes[at..at + LENGTH];
        let apart = |byte: Option<&u8>| byte.is_none_or(|byte| !byte.is_ascii_alphanumeric());
        apart(at.checked_sub(1).and_then(|before| bytes.get(before)))
            && apart(bytes.get(at + LENGTH))
            && candidate.iter().enumerate().all(|(i, byte)| match i {
                8 | 13 | 18 | 23 => *byte == b'-',
                _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(byte),
            })
    };
    let mut at = 0;
    std::iter::from_fn(move || {
        while at + LENGTH <= bytes.len() {
            if is_uuid(at) {
                let found = (at, &text[at..at + LENGTH]);
                at += LENGTH;
                return Some(found);
            }
            at += 1;
        }
        None
    })
}

/// A fixture's entries, split into each process's, each starting with its [`Entry::Launch`].
pub fn processes(fixture: &str) -> Result<Vec<Vec<Entry>>, String> {
    let mut processes: Vec<Vec<Entry>> = Vec::new();
    for (number, line) in fixture.lines().enumerate().filter(|(_, line)| !line.trim().is_empty()) {
        let entry: Entry = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", number + 1))?;
        match (&entry, processes.last_mut()) {
            (Entry::Launch { .. }, _) => processes.push(vec![entry]),
            (_, Some(process)) => process.push(entry),
            (_, None) => return Err(format!("line {}: an entry before any launch", number + 1)),
        }
    }
    Ok(processes)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Entry, fixture};
    use crate::conformance::lines::Lines;

    /// Lines whose `type` is their kind, where an `ask` is a request the adapter makes by itself, with a `request_id`.
    struct Typed;

    impl Lines for Typed {
        fn kind(&self, line: &Value) -> String {
            line["type"].as_str().unwrap_or_default().to_string()
        }

        fn fresh_ids(&self, line: &Value) -> &'static [&'static [&'static str]] {
            if self.by_itself(line) { &[&["request_id"]] } else { &[] }
        }

        fn by_itself(&self, line: &Value) -> bool {
            line["type"] == "ask"
        }

        fn answers<'a>(&self, _: &'a Value) -> Option<&'a str> {
            None
        }

        fn answer(&self, _: Option<&Value>, _: &Value) -> Value {
            Value::Null
        }
    }

    #[test]
    fn a_fixture_numbers_each_id_where_it_first_appears_and_each_set_apart_in_a_range_of_its_own() {
        let (prompt, asked, reply) = (
            "dc55a3a8-9bd0-48f6-baa4-4bac05623980",
            "37d5a0a3-48f4-4b53-aeda-5f5d1434bcdb",
            "048bb47c-5317-495c-988d-a0a8819ba0d1",
        );
        let entries = [
            Entry::ToCli(json!({"uuid": prompt})),
            Entry::ToCli(json!({"type": "ask", "request_id": asked})),
            Entry::FromCli(json!({
                "uuid": reply,
                "listed": format!("one:\n{prompt}.jsonl"),
                "not": [format!("x{reply}"), format!("{reply}x")],
            })),
        ];

        let text = fixture(&entries, &[], &Typed);

        let lines: Vec<serde_json::Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(lines[0]["to_cli"]["uuid"], "00000000-0000-4000-8000-000000000001");
        assert_eq!(lines[1]["to_cli"]["request_id"], "00000000-0000-4000-8001-000000000001");
        let from_cli = &lines[2]["from_cli"];
        assert_eq!(from_cli["uuid"], "00000000-0000-4000-8000-000000000002");
        assert_eq!(from_cli["listed"], "one:\n00000000-0000-4000-8000-000000000001.jsonl");
        assert_eq!(from_cli["not"], json!([format!("x{reply}"), format!("{reply}x")]));
    }
}
