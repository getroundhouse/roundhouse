//! A subagent, started in the background as the real CLI starts one: the call's result only says it started, and
//! the subagent's own calls and report come later, naming the call as their parent. It makes one call, or as many as
//! it's asked for, for a subagent tab long enough to scroll. Its report streams in a few pieces, and one that works
//! until a file exists writes the first and holds the rest until then, so a test can see what it's writing, and cut it
//! off. The CLI's word on the task it runs as comes around that, as the real CLI's does: that it started, after the
//! calls, how many tokens it's using, after each of its own calls, and that it finished, after its report, with the
//! count then. In the background, its task is listed the line before it starts, and left out after it finishes. Its
//! work is part of the turn here, so stopping the turn ends it, and the list leaves it out then too, though the real
//! CLI's would carry on. One in the foreground is the same, but for the call's result, which comes last, with the
//! report, and it isn't listed.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::Receiver;

use serde_json::json;

use crate::turn::{Block, Turn, Within};
use crate::waiting::pause_until_made;
use crate::{Input, Prompt, background, emit};

/// How many tokens each of its calls adds to what the CLI says it's using, and its report on top of them.
const TOKENS_A_CALL: usize = 1_200;
const TOKENS_TO_REPORT: usize = 300;

/// The subagent a command asks for.
pub(crate) struct Asked<'a> {
    description: &'a str,
    in_background: bool,
    calls: usize,
    /// The file it holds the rest of its report for, once it has written the first piece, if it waits on one.
    until: Option<&'a Path>,
}

impl<'a> Asked<'a> {
    pub(crate) fn read(command: &'a str) -> Option<Self> {
        let until = |rest: &'a str, in_background| {
            let (file, description) = rest.split_once(':')?;
            Some(Asked { description, in_background, calls: 1, until: Some(Path::new(file)) })
        };
        if let Some(rest) = command.strip_prefix("agent-until:") {
            return until(rest, true);
        }
        if let Some(rest) = command.strip_prefix("agent-foreground-until:") {
            return until(rest, false);
        }
        let (calls, description) = command.strip_prefix("agent-calls:")?.split_once(':')?;
        Some(Asked { description, in_background: true, calls: calls.parse().unwrap(), until: None })
    }
}

/// Returns whether the turn was interrupted while the subagent worked.
pub(crate) fn start(
    turn: &mut Turn,
    asked: &Asked,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let Asked { description, in_background, calls, until } = *asked;
    // It's asked to do what it's for, as a sentence.
    let prompt = format!("{description}.");
    let input = json!({"description":description,"subagent_type":"Explore","prompt":prompt});
    let agent = turn.named_tool_use("Agent", input);
    let (task, session_id) = (format!("task_{agent}"), &turn.config.session_id);
    if in_background {
        background::listed(turn.config, turn.out, &task, "local_agent", description);
    }
    emit(
        turn.out,
        json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":agent,"description":description,
        "task_type":"local_agent","subagent_type":"Explore","prompt":prompt,"is_backgrounded":in_background,
        "session_id":session_id}),
    );
    if in_background {
        turn.tool_result(&agent, "Async agent launched successfully.", false);
    }
    let mut within = Turn { within: Some(Within { call: &agent, task: &task }), ..*turn };
    for made in 1..=calls {
        let call = within.named_tool_use("Bash", json!({"command":"ls","description":"List what's here"}));
        within.tool_result(&call, "README.md", false);
        // As the real CLI says how far each of its tasks has got, a subagent's with how many tokens it's using.
        emit(
            turn.out,
            json!({"type":"system","subtype":"task_progress","task_id":task,"tool_use_id":agent,
            "description":"Running List what's here","subagent_type":"Explore",
            "usage":{"total_tokens":TOKENS_A_CALL * made,"tool_uses":made,"duration_ms":1},"last_tool_name":"Bash",
            "session_id":session_id}),
        );
    }
    let mut report = within.start_writing(Block::Text);
    within.write(&mut report, "There's ".to_string());
    if until.is_some_and(|file| pause_until_made(file, rx, queued, eof)) {
        if in_background {
            background::unlisted(turn.config, turn.out, &task);
        }
        return true;
    }
    within.write(&mut report, "a README ".to_string());
    within.write(&mut report, "here.".to_string());
    within.finish_writing(report);
    emit(
        turn.out,
        json!({"type":"system","subtype":"task_notification","task_id":task,"tool_use_id":agent,"status":"completed",
        "summary":"There's a README here.",
        "usage":{"total_tokens":TOKENS_A_CALL * calls + TOKENS_TO_REPORT,"tool_uses":calls,"duration_ms":1},
        "session_id":session_id}),
    );
    if in_background {
        background::unlisted(turn.config, turn.out, &task);
    } else {
        turn.tool_result(&agent, "There's a README here.", false);
    }
    false
}
