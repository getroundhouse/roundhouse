//! What a subagent has done lately, which its call shows under `Working on` while it works, as the blocks to draw:
//! each of its newest calls after what it wrote just before it, then what it's written since, as its record has them.

use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::CommandOutput;
use protocol::event::ContentBlock;
use protocol::model::{LatelyCall, Said, Subagent as Record};

use crate::item_key::ItemKey;

/// One block of what it's done lately.
#[derive(Debug, PartialEq)]
pub(super) struct Lately {
    pub(super) block: ContentBlock,
    pub(super) key: ItemKey,
    /// For a call: what came back for it, as a result block, when that's drawn.
    pub(super) result: Option<ContentBlock>,
}

/// Oldest first. A subagent that hasn't done anything yet, or has ended, has nothing it reads.
pub(super) fn lately(record: &Record) -> Vec<Lately> {
    let Some(work) = &record.lately else { return Vec::new() };
    let mut blocks = Vec::new();
    for made in &work.calls {
        blocks.extend(made.before.as_ref().map(|said| wrote(said, ItemKey::Before(made.id.clone()))));
        blocks.push(called(made));
    }
    // What it's written since its newest call, or since it started, if it's made none.
    let since =
        work.calls.last().map_or_else(|| ItemKey::Since(record.call.clone()), |made| ItemKey::After(made.id.clone()));
    blocks.extend(work.since.as_ref().map(|said| wrote(said, since)));
    blocks
}

/// A call, with what came back for it when that's whole, or says what went wrong. Its line counts what came back,
/// such as a read's lines, so the start of a long one would say it read fewer than it did, and the call is drawn as
/// one still waiting instead.
fn called(made: &LatelyCall) -> Lately {
    let block = ContentBlock::Call { id: made.id.clone(), call: made.call.clone() };
    let result = made.result.as_ref().filter(|result| !result.cut || result.is_error).map(|result| {
        let output = if result.cut { started(result.output.clone()) } else { result.output.clone() };
        ContentBlock::CallResult {
            tool_use_id: made.id.clone(),
            output,
            is_error: result.is_error,
            denied: result.denied.clone(),
            images: result.images.clone(),
        }
    });
    Lately { block, key: ItemKey::Call(made.id.clone()), result }
}

/// What went wrong, of which `output` holds only the start, saying so where it's words, which are all a failure
/// shows, as [`ContentBlock::CallResult`] says.
fn started(output: CallOutput) -> CallOutput {
    let more = |text: String| format!("{}…", text.trim_end());
    match output {
        CallOutput::Command(command) => CallOutput::Command(CommandOutput { output: more(command.output), ..command }),
        CallOutput::Words { text } => CallOutput::Words { text: more(text) },
        CallOutput::Report { report } => CallOutput::Report { report: more(report) },
        other @ (CallOutput::Lines(_)
        | CallOutput::Image { .. }
        | CallOutput::Document { .. }
        | CallOutput::Changed { .. }
        | CallOutput::Found(_)
        | CallOutput::Paths(_)
        | CallOutput::Page { .. }
        | CallOutput::WebResults { .. }
        | CallOutput::Mcp { .. }
        | CallOutput::Encrypted
        | CallOutput::Backgrounded { .. }
        | CallOutput::Unknown) => other,
    }
}

fn wrote(said: &Said, key: ItemKey) -> Lately {
    let text = if said.cut { format!("{}…", said.text.trim_end()) } else { said.text.clone() };
    Lately { block: ContentBlock::Text { text, images: Vec::new() }, key, result: None }
}

#[cfg(test)]
mod tests {
    use protocol::capability::calls::call::Read;
    use protocol::capability::calls::output::Ended;
    use protocol::capability::calls::{Call, CallOutput};
    use protocol::event::ContentBlock;
    use protocol::model::{Lately as Work, LatelyCall, LatelyResult, Said, Subagent, SubagentStatus};

    use super::lately;
    use crate::item_key::ItemKey;

    fn said(text: &str, cut: bool) -> Said {
        Said { text: text.into(), cut }
    }

    /// Reading the file named `id`, with what came back for it, if anything has.
    fn read(id: &str, before: Option<Said>, result: Option<LatelyResult>) -> LatelyCall {
        let call = Call::Read(Read { path: format!("{id}.rs"), lines: None, pages: None });
        LatelyCall { before, id: id.into(), call, result }
    }

    fn came_back(output: CallOutput, cut: bool, is_error: bool) -> LatelyResult {
        LatelyResult { output, is_error, cut, images: Vec::new(), denied: None }
    }

    fn words(text: &str) -> CallOutput {
        CallOutput::Words { text: text.into() }
    }

    fn record(work: Option<Work>) -> Subagent {
        Subagent { calls: 4, lately: work, ..crate::fixtures::record("agent", SubagentStatus::Working) }
    }

    /// Each block as `(what it is, its key, what came back)`, to compare.
    fn shown(record: &Subagent) -> Vec<(String, ItemKey, Option<String>)> {
        let text_of = |block: &ContentBlock| match block {
            ContentBlock::Text { text, .. } => format!("text {text}"),
            ContentBlock::Call { id, .. } => format!("call {id}"),
            ContentBlock::CallResult { output, .. } => client_core::calls::typed::output(output),
            other @ (ContentBlock::Thinking { .. } | ContentBlock::Image { .. } | ContentBlock::Unknown) => {
                format!("{other:?}")
            }
        };
        lately(record).iter().map(|l| (text_of(&l.block), l.key.clone(), l.result.as_ref().map(text_of))).collect()
    }

    #[test]
    fn each_call_comes_after_what_it_wrote_before_it_then_what_it_wrote_since() {
        let calls = vec![
            read("b", Some(said("First this.", false)), Some(came_back(words("fn b"), false, false))),
            read("c", None, None),
            read("d", Some(said("Then", true)), None),
        ];
        let record = record(Some(Work { calls, since: Some(said("Nearly done.", false)) }));

        assert_eq!(
            shown(&record),
            [
                ("text First this.".into(), ItemKey::Before("b".into()), None),
                ("call b".into(), ItemKey::Call("b".into()), Some("fn b".into())),
                ("call c".into(), ItemKey::Call("c".into()), None),
                ("text Then…".into(), ItemKey::Before("d".into()), None),
                ("call d".into(), ItemKey::Call("d".into()), None),
                ("text Nearly done.".into(), ItemKey::After("d".into()), None),
            ]
        );
    }

    /// What went wrong is cut short saying so, as words or what a command printed.
    #[test]
    fn only_what_came_back_whole_or_went_wrong_is_drawn() {
        let failed = protocol::fixtures::command_output("No such file", Some(1), Ended::Exited);
        let command = Call::Command(protocol::fixtures::typed_command("cat gone.rs"));
        let calls = vec![
            read("long", None, Some(came_back(words("fn long"), true, false))),
            read("missing", None, Some(came_back(words("Not there"), true, true))),
            LatelyCall {
                before: None,
                id: "failed".into(),
                call: command,
                result: Some(came_back(failed, true, true)),
            },
        ];
        let record = record(Some(Work { calls, since: None }));

        let results: Vec<Option<String>> = shown(&record).into_iter().map(|(_, _, result)| result).collect();
        assert_eq!(
            results,
            [None, Some("Not there…".into()), Some("No such file…".into())],
            "a long read's start would count too few lines"
        );
    }

    #[test]
    fn what_it_wrote_before_any_call_follows_the_call_that_started_it() {
        let record = record(Some(Work { calls: Vec::new(), since: Some(said("Looking.", false)) }));

        assert_eq!(shown(&record), [("text Looking.".into(), ItemKey::Since("agent".into()), None)]);
    }

    #[test]
    fn a_record_that_keeps_nothing_lately_shows_nothing() {
        assert_eq!(shown(&record(None)), []);
    }
}
