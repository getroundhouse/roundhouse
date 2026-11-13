//! What a subagent has done lately, which its call shows under `Working on` while it works, as the blocks to draw:
//! each of its newest calls after what it wrote just before it, then what it's written since, as its record has them.

use protocol::event::ContentBlock;
use protocol::model::{Call, CallResult, Said, Subagent as Record};

use crate::item_key::ItemKey;

/// One block of what it's done lately.
#[derive(Debug, PartialEq)]
pub(super) struct Lately {
    pub(super) block: ContentBlock,
    pub(super) key: ItemKey,
    /// For a call: what came back for it, as a result block, when that's drawn.
    pub(super) result: Option<ContentBlock>,
}

/// Oldest first. A record from a hub from before it kept what a subagent's done lately has only its newest call.
pub(super) fn lately(record: &Record) -> Vec<Lately> {
    let work = &record.recent;
    if work.calls.is_empty() && work.since.is_none() {
        return record.latest_call.iter().map(|call| called(call, None)).collect();
    }
    let mut blocks = Vec::new();
    for made in &work.calls {
        blocks.extend(made.before.as_ref().map(|said| wrote(said, ItemKey::Before(made.call.id.clone()))));
        blocks.push(called(&made.call, made.result.as_ref()));
    }
    // What it's written since its newest call, or since it started, if it's made none.
    let since = work
        .calls
        .last()
        .map_or_else(|| ItemKey::Since(record.call.clone()), |made| ItemKey::After(made.call.id.clone()));
    blocks.extend(work.since.as_ref().map(|said| wrote(said, since)));
    blocks
}

fn wrote(said: &Said, key: ItemKey) -> Lately {
    let text = if said.cut { format!("{}…", said.text.trim_end()) } else { said.text.clone() };
    Lately { block: ContentBlock::Text { text, images: Vec::new() }, key, result: None }
}

/// A call, with what came back for it when that's whole, or says what went wrong. Its line counts what came back,
/// such as a read's lines, so the start of a long one would say it read fewer than it did, and the call is drawn as
/// one still waiting instead.
fn called(call: &Call, result: Option<&CallResult>) -> Lately {
    let (id, name, input) = (call.id.clone(), call.name.clone(), call.input.clone());
    let block = ContentBlock::ToolUse { id, name, input, call: call.kind.clone() };
    let result = result.filter(|result| !result.cut || result.is_error).map(|result| ContentBlock::ToolResult {
        tool_use_id: call.id.clone(),
        is_error: result.is_error,
        // Only the start of what went wrong, which says so.
        text: if result.cut { format!("{}…", result.text.trim_end()) } else { result.text.clone() },
        images: result.images.clone(),
        change: None,
        encrypted: result.encrypted,
        // An adapter that says what kind of call each is says it on every result too, so the result says one exactly
        // when its call does, and with the call's it's read as it was recorded.
        call: call.kind.clone(),
        exit_code: result.exit_code,
        denied: result.denied.clone(),
    });
    Lately { block, key: ItemKey::Call(call.id.clone()), result }
}

#[cfg(test)]
mod tests {
    use protocol::event::{CallKind, ContentBlock};
    use protocol::model::{Call, CallResult, RecentCall, RecentWork, Said, Subagent, SubagentStatus};
    use serde_json::json;

    use super::{Lately, lately};
    use crate::item_key::ItemKey;

    fn call(id: &str) -> Call {
        protocol::fixtures::call(id, "Read", json!({ "file_path": format!("{id}.rs") }))
    }

    fn said(text: &str, cut: bool) -> Said {
        Said { text: text.into(), cut }
    }

    fn came_back(text: &str, cut: bool, is_error: bool) -> CallResult {
        CallResult { is_error, cut, ..protocol::fixtures::call_result(text) }
    }

    fn record(recent: RecentWork, latest: Option<Call>) -> Subagent {
        Subagent { calls: 4, latest_call: latest, recent, ..crate::fixtures::record("agent", SubagentStatus::Working) }
    }

    /// Each block as `(what it is, its key, what came back)`, to compare.
    fn shown(record: &Subagent) -> Vec<(String, ItemKey, Option<String>)> {
        let text_of = |block: &ContentBlock| match block {
            ContentBlock::Text { text, .. } => format!("text {text}"),
            ContentBlock::ToolUse { id, .. } => format!("call {id}"),
            ContentBlock::ToolResult { text, .. } => text.clone(),
            other @ (ContentBlock::Thinking { .. } | ContentBlock::Image { .. } | ContentBlock::Unknown) => {
                format!("{other:?}")
            }
        };
        lately(record).iter().map(|l| (text_of(&l.block), l.key.clone(), l.result.as_ref().map(text_of))).collect()
    }

    #[test]
    fn each_call_comes_after_what_it_wrote_before_it_then_what_it_wrote_since() {
        let calls = vec![
            RecentCall {
                before: Some(said("First this.", false)),
                call: call("b"),
                result: Some(came_back("1\tfn b", false, false)),
            },
            RecentCall { before: None, call: call("c"), result: None },
            RecentCall { before: Some(said("Then", true)), call: call("d"), result: None },
        ];
        let record = record(RecentWork { calls, since: Some(said("Nearly done.", false)) }, Some(call("d")));

        assert_eq!(
            shown(&record),
            [
                ("text First this.".into(), ItemKey::Before("b".into()), None),
                ("call b".into(), ItemKey::Call("b".into()), Some("1\tfn b".into())),
                ("call c".into(), ItemKey::Call("c".into()), None),
                ("text Then…".into(), ItemKey::Before("d".into()), None),
                ("call d".into(), ItemKey::Call("d".into()), None),
                ("text Nearly done.".into(), ItemKey::After("d".into()), None),
            ]
        );
    }

    #[test]
    fn only_what_came_back_whole_or_went_wrong_is_drawn() {
        let calls = vec![
            RecentCall { before: None, call: call("long"), result: Some(came_back("1\tfn long", true, false)) },
            RecentCall { before: None, call: call("failed"), result: Some(came_back("No such file", true, true)) },
        ];
        let record = record(RecentWork { calls, since: None }, None);

        let results: Vec<Option<String>> = shown(&record).into_iter().map(|(_, _, result)| result).collect();
        assert_eq!(results, [None, Some("No such file…".into())], "a long read's start would count too few lines");
    }

    #[test]
    fn what_it_wrote_before_any_call_follows_the_call_that_started_it() {
        let record = record(RecentWork { calls: Vec::new(), since: Some(said("Looking.", false)) }, None);

        assert_eq!(shown(&record), [("text Looking.".into(), ItemKey::Since("agent".into()), None)]);
    }

    #[test]
    fn a_record_from_a_hub_from_before_shows_its_newest_call() {
        let record = record(RecentWork::default(), Some(call("d")));

        assert_eq!(shown(&record), [("call d".into(), ItemKey::Call("d".into()), None)]);
    }

    /// So a subagent's work draws from its record as it does from its events: by the kind each call says, and with
    /// how a failed command exited beside what it printed.
    #[test]
    fn a_call_and_what_came_back_keep_the_kind_and_exit_code_the_record_has() {
        let said = Some(CallKind::Command);
        let made = Call { kind: said.clone(), ..protocol::fixtures::call("c", "shell", json!({ "command": "make" })) };
        let failed = CallResult { is_error: true, exit_code: Some(2), ..protocol::fixtures::call_result("no rule") };
        let calls = vec![RecentCall { before: None, call: made, result: Some(failed) }];

        let shown = lately(&record(RecentWork { calls, since: None }, None));

        let [Lately { block: ContentBlock::ToolUse { call: made, .. }, result: Some(result), .. }] = shown.as_slice()
        else {
            panic!("one call, with its result")
        };
        let ContentBlock::ToolResult { call, exit_code, .. } = result else { panic!("a result") };
        assert_eq!((made, call, *exit_code), (&said, &said, Some(2)));
    }
}
