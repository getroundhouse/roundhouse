//! What a capture's lines are, in an agent's own protocol, which its binding says: what kind each is, which IDs the
//! adapter makes afresh, and which requests it makes by itself. A fixture and a replay both read a capture by them.

use serde_json::Value;

use super::capture::Entry;

/// What a fixture and a replay need to know of the lines an agent's CLI and its adapter write each other, which is
/// the agent's own protocol, so its binding says.
pub trait Lines: Send + Sync {
    /// What kind of line it is, by which a line the adapter writes is matched with one in the capture, such as
    /// `control_request/interrupt`.
    fn kind(&self, line: &Value) -> String;

    /// Where a line the adapter writes holds IDs it makes afresh each time, such as a prompt's, as paths of keys.
    fn fresh_ids(&self, line: &Value) -> &'static [&'static [&'static str]];

    /// Whether the adapter writes `line`, a request, by itself, at a time of its own rather than at a step's, so it's
    /// answered whenever it comes, with what the CLI answered the same kind in the capture.
    fn by_itself(&self, line: &Value) -> bool;

    /// The ID of the adapter's request that `line`, the CLI's, answers, if it answers one.
    fn answers<'a>(&self, line: &'a Value) -> Option<&'a str>;

    /// `captured`, the CLI's answer to a request like `request` in the capture, made the answer to `request`, or an
    /// answer saying the capture has none.
    fn answer(&self, captured: Option<&Value>, request: &Value) -> Value;
}

/// Each ID of each request the adapter made by itself in `entries`, with the request's kind, in the order it made them.
pub fn asked_by_itself<'a>(lines: &dyn Lines, entries: &'a [Entry]) -> Vec<(String, &'a str)> {
    let mut asked = Vec::new();
    for entry in entries {
        if let Entry::ToCli(line) = entry
            && lines.by_itself(line)
        {
            asked.extend(ids_in(line, lines.fresh_ids(line)).into_iter().map(|id| (lines.kind(line), id)));
        }
    }
    asked
}

/// The strings at `paths` in `line`, in order, leaving out those it doesn't have.
pub(crate) fn ids_in<'a>(line: &'a Value, paths: &[&[&str]]) -> Vec<&'a str> {
    let at = |path: &&[&str]| path.iter().try_fold(line, |value, key| value.get(key)).and_then(Value::as_str);
    paths.iter().filter_map(at).collect()
}
