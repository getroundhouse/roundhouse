//! A subagent, started in the background as the real CLI starts one: the call's result only says it started, and
//! the subagent's own calls and report come later, naming the call as their parent. It makes one call, or as many as
//! it's asked for, for a subagent tab long enough to scroll. Its report streams in a few pieces, so a test can see
//! what it's writing, and cut it off. The CLI's word on the task it runs as comes around that, as the real CLI's does:
//! that it started, after the calls, how many tokens it's using, after each of its own calls, and that it finished,
//! after its report, with the count then. In the background, its task is listed the line before it starts, and left
//! out after it finishes. Its work is part of the turn here, so stopping the turn ends it, and the list leaves it out
//! then too, though the real CLI's would carry on. One in the foreground is the same, but for the call's result, which
//! comes last, with the report, and it isn't listed.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::json;

use crate::turn::{Block, Turn};
use crate::waiting::WaitingOn;
use crate::{Input, Prompt, background, emit, pause};

/// How long a subagent takes before its work arrives, so a test can open its call first.
const WORKS_FOR: Duration = Duration::from_millis(1000);

/// Between the pieces of its report, so a test can catch it part way.
const WRITES_EVERY: Duration = Duration::from_millis(200);

/// How many tokens each of its calls adds to what the CLI says it's using, and its report on top of them.
const TOKENS_A_CALL: usize = 1_200;
const TOKENS_TO_REPORT: usize = 300;

/// How often one that works until a file exists looks for it.
const POLL: Duration = Duration::from_millis(20);

/// The subagent a command asks for.
pub(crate) struct Asked<'a> {
    description: &'a str,
    in_background: bool,
    calls: usize,
    pace: Pace<'a>,
}

/// How long a subagent works before its work arrives, and how its report is written.
#[derive(Clone, Copy)]
enum Pace<'a> {
    /// For [`WORKS_FOR`], and its report in pieces [`WRITES_EVERY`].
    Timed,
    /// Until the file exists, and its report as [`Pace::Timed`]'s is.
    Until(&'a Path),
    /// Not at all, and its report at once.
    AtOnce,
}

impl Pace<'_> {
    /// Waits as this says before the subagent's work arrives. Whether the turn was interrupted meanwhile.
    fn works(self, rx: &Receiver<Input>, queued: &mut VecDeque<Prompt>, eof: &mut bool) -> bool {
        match self {
            Pace::Timed => pause(rx, WORKS_FOR, queued, eof),
            Pace::Until(file) => {
                let waiting = WaitingOn::file(file);
                while !waiting.made() {
                    if pause(rx, POLL, queued, eof) {
                        return true;
                    }
                }
                false
            }
            Pace::AtOnce => false,
        }
    }

    /// How long between the pieces of its report.
    fn writes_every(self) -> Duration {
        match self {
            Pace::Timed | Pace::Until(_) => WRITES_EVERY,
            Pace::AtOnce => Duration::ZERO,
        }
    }
}

impl<'a> Asked<'a> {
    pub(crate) fn read(command: &'a str) -> Option<Self> {
        let asked = |description, in_background, calls| Asked { description, in_background, calls, pace: Pace::Timed };
        if let Some(description) = command.strip_prefix("agent:") {
            return Some(asked(description, true, 1));
        }
        if let Some(description) = command.strip_prefix("agent-foreground:") {
            return Some(asked(description, false, 1));
        }
        if let Some((file, description)) = command.strip_prefix("agent-until:").and_then(|rest| rest.split_once(':')) {
            return Some(Asked { pace: Pace::Until(Path::new(file)), ..asked(description, true, 1) });
        }
        let (calls, description) = command.strip_prefix("agent-calls:")?.split_once(':')?;
        Some(Asked { pace: Pace::AtOnce, ..asked(description, true, calls.parse().unwrap()) })
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
    let Asked { description, in_background, calls, pace } = *asked;
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
    let stopped = |turn: &Turn| {
        if in_background {
            background::unlisted(turn.config, turn.out, &task);
        }
        true
    };
    if pace.works(rx, queued, eof) {
        return stopped(turn);
    }
    let mut within = Turn { parent: Some(&agent), ..*turn };
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
    let report = ["There's ", "a README ", "here."].map(String::from).to_vec();
    if within.say(Block::Text, report, pace.writes_every(), rx, queued, eof) {
        return stopped(turn);
    }
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
