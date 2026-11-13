//! Asking the CLI what's left of its account's limits, since it says so by itself only once a process, and reporting
//! what it answers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use protocol::event::EventBody;
use protocol::model::UsageLimit;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::debug;

use super::asking::{Over, Work, work};
use super::{Shared, control};
use crate::claude::usage::{lists_every_models_week, usage_limits};
use crate::{AgentError, AgentOutput};

/// The least time between two words of the account's limits, the CLI's own or an answer to an ask. The CLI writes a
/// `rate_limit_event` line only in a process's first turn, and `get_usage` takes over a second and answers with much
/// else, so it's asked at most this often, while the agent works.
const GAP: Duration = Duration::from_mins(5);

/// Where asking has got to.
#[derive(Default)]
pub(super) struct UsageWatch {
    /// The newest word of the limits, whether the CLI said it by itself or was asked.
    last: Option<Instant>,
    /// An ask is on its way, so another isn't made.
    asking: bool,
    /// Why nothing more is asked or reported, once that's so.
    over: Option<Over>,
}

impl UsageWatch {
    /// The CLI said what's left by itself, at `now`, so nothing is asked until the gap has passed.
    fn heard(&mut self, now: Instant) {
        self.last = Some(now);
    }

    /// Whether to ask now. With nothing said yet, a turn's end asks at once, since a CLI that hasn't said by itself
    /// by the end of its first turn won't. After that, only once the gap has passed since the newest word.
    fn begin(&mut self, turn_ended: bool, now: Instant) -> bool {
        if self.over.is_some() || self.asking {
            return false;
        }
        let due = self.last.map_or(turn_ended, |at| now.duration_since(at) >= GAP);
        if due {
            self.asking = true;
            self.last = Some(now);
        }
        due
    }

    /// The ask is over, and this is what it brought. What to report: the limits it names, unless there are none, as
    /// for an account the CLI says has none to tell of just now, or the process has exited, after which nothing is.
    fn answered(&mut self, answer: Result<Vec<UsageLimit>, AgentError>) -> Option<Vec<UsageLimit>> {
        self.asking = false;
        match answer {
            Ok(limits) => (!limits.is_empty() && self.over != Some(Over::Exited)).then_some(limits),
            Err(e) => {
                if let Some(over) = Over::after(&e) {
                    self.over.get_or_insert(over);
                }
                None
            }
        }
    }

    /// The process was launched again, and the new one says by itself in its first turn, as the first did.
    pub(super) fn relaunched(&mut self) {
        self.last = None;
    }

    /// Nothing more is reported. The supervisor calls this before it says the process exited, which has to be the
    /// last thing said.
    pub(super) fn close(&mut self) {
        self.over = Some(Over::Exited);
    }
}

/// Notes the CLI's own word of its limits, and asks after anything else that's a reason to, when it's due. The
/// answer comes through the loop that calls this, so the ask runs on a task of its own.
pub(super) fn after(shared: &Arc<Shared>, body: &EventBody, output: &mpsc::Sender<AgentOutput>) {
    let now = Instant::now();
    let work = work(body);
    if work == Work::HeardLimits {
        shared.usage.lock().heard(now);
        return;
    }
    let Some(turn_ended) = asks_after(work) else { return };
    if !shared.usage.lock().begin(turn_ended, now) {
        return;
    }
    let (shared, output) = (shared.clone(), output.clone());
    tokio::spawn(async move {
        let answer = control(&shared, json!({"subtype": "get_usage"})).await;
        let every_models_week = answer.as_ref().is_ok_and(lists_every_models_week);
        report(&shared, &output, answer.map(|reply| usage_limits(&reply)), every_models_week);
    });
}

/// Whether `work` is a reason to ask, if it's due, and if so whether a turn ended. Anything the agent or one of its
/// subagents wrote is one: a subagent's work is the account's use as much as the agent's, and the agent may write
/// nothing for as long as one works.
fn asks_after(work: Work) -> Option<bool> {
    match work {
        Work::TurnEnded => Some(true),
        Work::Wrote { .. } => Some(false),
        Work::Started | Work::HeardLimits | Work::Other => None,
    }
}

/// Reports what the CLI answered, which is `every_models_week` the account has when it lists them all. It's sent under
/// the lock, without waiting, so it can't come after the process's exit, and a report there's no room for is dropped,
/// since the next ask brings another.
fn report(
    shared: &Shared,
    output: &mpsc::Sender<AgentOutput>,
    answer: Result<Vec<UsageLimit>, AgentError>,
    every_models_week: bool,
) {
    if let Err(e) = &answer {
        debug!("the agent didn't say what's left of its account's limits: {e}");
    }
    let mut watch = shared.usage.lock();
    if let Some(limits) = watch.answered(answer) {
        let _ = output.try_send(AgentOutput::Limits { limits, every_models_week });
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::LimitWindow;

    use super::*;

    fn the_week() -> Vec<UsageLimit> {
        vec![UsageLimit {
            window: LimitWindow::SevenDays,
            model: None,
            used_percent: 4,
            resets_at_ms: None,
            heard_at_ms: None,
        }]
    }

    #[test]
    fn with_nothing_said_a_turns_end_asks_and_nothing_else_does() {
        let mut watch = UsageWatch::default();
        let start = Instant::now();

        assert!(!watch.begin(false, start), "the agent's own words don't, while its first turn may still say");
        assert!(watch.begin(true, start), "a first turn that ended without saying");
    }

    #[test]
    fn the_clis_own_word_puts_off_asking_until_the_gap_has_passed() {
        let mut watch = UsageWatch::default();
        let start = Instant::now();
        watch.heard(start);

        assert!(!watch.begin(true, start), "the turn's end after it said");
        assert!(!watch.begin(false, start + GAP / 2));
        assert!(watch.begin(false, start + GAP), "five minutes on");
    }

    #[test]
    fn an_ask_puts_off_the_next_until_the_gap_has_passed_and_one_is_asked_at_a_time() {
        let mut watch = UsageWatch::default();
        let start = Instant::now();
        assert!(watch.begin(true, start));

        assert!(!watch.begin(true, start + GAP), "one ask at a time");
        watch.answered(Ok(the_week()));
        assert!(!watch.begin(true, start + GAP / 2), "too soon after the last");
        assert!(watch.begin(false, start + GAP));
    }

    #[test]
    fn a_new_launch_is_left_to_say_by_itself_first() {
        let mut watch = UsageWatch::default();
        let start = Instant::now();
        watch.heard(start);

        watch.relaunched();

        assert!(!watch.begin(false, start), "its first turn may say");
        assert!(watch.begin(true, start), "and once it ends without saying, it's asked");
    }

    #[test]
    fn what_the_agent_or_a_subagent_writes_and_a_turns_end_are_reasons_to_ask() {
        assert_eq!(asks_after(Work::Wrote { subagent: false }), Some(false));
        assert_eq!(asks_after(Work::Wrote { subagent: true }), Some(false), "the agent may write nothing meanwhile");
        assert_eq!(asks_after(Work::TurnEnded), Some(true));
        assert_eq!(asks_after(Work::Started), None, "its first turn may still say by itself");
    }

    #[test]
    fn an_answer_is_reported_unless_it_names_no_limit_or_the_process_has_exited() {
        let mut watch = UsageWatch::default();
        let start = Instant::now();
        watch.begin(true, start);

        assert_eq!(watch.answered(Ok(the_week())), Some(the_week()));
        assert_eq!(watch.answered(Ok(Vec::new())), None, "as for an account with none to tell of just now");
        assert!(watch.begin(false, start + GAP), "which is asked again all the same");
        watch.close();
        assert_eq!(watch.answered(Ok(the_week())), None, "the ask that was on its way when the process exited");
        assert!(!watch.begin(true, start + GAP * 2));
    }

    #[test]
    fn only_a_cli_that_doesnt_know_the_request_is_never_asked_again() {
        let start = Instant::now();
        let mut unanswered = UsageWatch::default();
        let mut unknown = UsageWatch::default();
        unanswered.begin(true, start);
        unknown.begin(true, start);

        assert_eq!(unanswered.answered(Err(AgentError::NoAnswer)), None);
        assert_eq!(unknown.answered(Err(AgentError::UnknownRequest("get_usage".to_string()))), None);

        assert!(unanswered.begin(false, start + GAP), "a failure that may not come twice");
        assert!(!unknown.begin(true, start + GAP));
    }
}
