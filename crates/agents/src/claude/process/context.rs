//! Asking the CLI how full its context is as it works, and reporting what it answers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use protocol::event::EventBody;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::debug;

use super::asking::{Over, Work, work};
use super::{Shared, control};
use crate::claude::context::{ModelWindows, context_usage};
use crate::claude::normalize::value_of;
use crate::{AgentError, AgentOutput};

/// The least time between two asks in the middle of a turn. Each report that differs is an event in the session's
/// log, and a turn's requests to the model can come every few seconds, so this keeps them to a few a minute while
/// the ring still moves as the agent works.
const MID_TURN_GAP: Duration = Duration::from_secs(15);

/// Where asking has got to.
#[derive(Default)]
pub(super) struct ContextWatch {
    asked_at: Option<Instant>,
    ask: Ask,
    /// Why nothing more is asked or reported, once that's so.
    over: Option<Over>,
    /// How much each model holds, which every report says of its own.
    windows: ModelWindows,
}

/// Whether the CLI is being asked now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Ask {
    #[default]
    None,
    OnItsWay,
    /// A turn ended while an ask was on its way, so what that one answers may be from before the end, and it's
    /// asked once more.
    OnItsWayAndWantedAgain,
}

impl ContextWatch {
    /// Whether to start asking now. A turn's end always asks, since that's what the context holds until the next
    /// turn. Anything else waits out [`MID_TURN_GAP`].
    fn begin(&mut self, turn_ended: bool, now: Instant) -> bool {
        if self.over.is_some() {
            return false;
        }
        match self.ask {
            Ask::None => {}
            Ask::OnItsWay if turn_ended => {
                self.ask = Ask::OnItsWayAndWantedAgain;
                return false;
            }
            Ask::OnItsWay | Ask::OnItsWayAndWantedAgain => return false,
        }
        let due = self.asked_at.is_none_or(|at| now.duration_since(at) >= MID_TURN_GAP);
        if !turn_ended && !due {
            return false;
        }
        self.ask = Ask::OnItsWay;
        self.asked_at = Some(now);
        true
    }

    /// The ask is over. Whether to ask once more, because a turn ended while it was on its way.
    fn finish(&mut self) -> bool {
        let again = self.ask == Ask::OnItsWayAndWantedAgain && self.over.is_none();
        self.ask = if again { Ask::OnItsWay } else { Ask::None };
        again
    }

    /// The process was launched again, as a launch that fails is, once. What the failed one was asked was about
    /// another process, so the new one is asked as if for the first time.
    pub(super) fn relaunched(&mut self) {
        self.asked_at = None;
    }

    /// Nothing more is reported. The supervisor calls this before it says the process exited, which has to be the
    /// last thing said.
    pub(super) fn close(&mut self) {
        self.over = Some(Over::Exited);
    }
}

/// Notes how much each model holds from the `line` a turn's end came in, the only line that says, so the report its
/// end asks for has it.
pub(super) fn heard(shared: &Shared, body: &EventBody, line: &str) {
    if !matches!(body, EventBody::TurnEnded { .. }) {
        return;
    }
    if let Some(result) = value_of(line) {
        shared.context.lock().windows.heard(&result);
    }
}

/// Asks after an event that says the context has moved: the start of a turn, something the agent itself wrote, and
/// a turn's end. The answer comes through the loop that calls this, so the ask runs on a task of its own.
pub(super) fn after(shared: &Arc<Shared>, body: &EventBody, output: &mpsc::Sender<AgentOutput>) {
    let turn_ended = match work(body) {
        Work::TurnEnded => true,
        Work::Started | Work::Wrote { subagent: false } => false,
        // A subagent's work is in a context of its own.
        Work::Wrote { subagent: true } | Work::HeardLimits | Work::Other => return,
    };
    if !shared.context.lock().begin(turn_ended, Instant::now()) {
        return;
    }
    let (shared, output) = (shared.clone(), output.clone());
    tokio::spawn(async move {
        loop {
            let answer = control(&shared, json!({"subtype": "get_context_usage"})).await;
            report(&shared, &output, answer);
            if !shared.context.lock().finish() {
                break;
            }
        }
    });
}

/// Reports what the CLI answered. It's sent under the lock, without waiting, so it can't come after the process's
/// exit, and a report there's no room for is dropped, since only the newest matters and the next ask brings one.
/// Whether it's a change is the hub's to say, which knows what the session's record has.
fn report(shared: &Shared, output: &mpsc::Sender<AgentOutput>, answer: Result<Value, AgentError>) {
    let mut watch = shared.context.lock();
    let context = match answer {
        Ok(reply) => context_usage(&reply, &watch.windows),
        Err(e) => {
            debug!("the agent didn't say how full its context is: {e}");
            if let Some(over) = Over::after(&e) {
                watch.over.get_or_insert(over);
            }
            None
        }
    };
    if let Some(context) = context
        && watch.over != Some(Over::Exited)
    {
        let _ = output.try_send(AgentOutput::Context(context));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turns_end_always_asks_and_anything_else_waits_out_the_gap() {
        let mut watch = ContextWatch::default();
        let start = Instant::now();

        assert!(watch.begin(false, start), "the first ask");
        assert!(!watch.finish());
        assert!(!watch.begin(false, start + MID_TURN_GAP / 2), "too soon after the last");
        assert!(watch.begin(true, start + MID_TURN_GAP / 2), "a turn's end");
        assert!(!watch.finish());
        assert!(!watch.begin(false, start + MID_TURN_GAP), "the gap counts from the newest ask");
        assert!(watch.begin(false, start + MID_TURN_GAP * 2));
    }

    #[test]
    fn a_turn_that_ends_during_an_ask_asks_once_more() {
        let mut watch = ContextWatch::default();
        let start = Instant::now();
        assert!(watch.begin(false, start));

        assert!(!watch.begin(false, start + MID_TURN_GAP), "one ask at a time");
        assert!(!watch.begin(true, start), "the ask on its way is asked again instead");

        assert!(watch.finish(), "asks again");
        assert!(!watch.begin(false, start + MID_TURN_GAP), "still asking");
        assert!(!watch.finish(), "and then is done");
        assert!(watch.begin(true, start));
    }

    #[test]
    fn nothing_is_asked_of_a_cli_that_cant_say_or_has_exited() {
        let start = Instant::now();
        let mut unsupported = ContextWatch { over: Some(Over::Unsupported), ..ContextWatch::default() };
        let mut closed = ContextWatch::default();
        assert!(closed.begin(true, start));
        assert!(!closed.begin(true, start));
        closed.close();

        assert!(!unsupported.begin(true, start));
        assert!(!closed.finish(), "the ask that was wanted again isn't made");
        assert!(!closed.begin(true, start));
    }

    #[test]
    fn a_launch_thats_replaced_leaves_the_new_process_to_be_asked_at_once() {
        let mut watch = ContextWatch::default();
        let start = Instant::now();
        assert!(watch.begin(true, start), "the failed launch's end");
        assert!(!watch.finish());
        assert!(!watch.begin(false, start), "too soon to ask the same process again");

        watch.relaunched();

        assert!(watch.begin(false, start), "the new process's first turn");
    }
}
