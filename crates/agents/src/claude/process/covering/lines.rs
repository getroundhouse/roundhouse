//! What a cover line is said for, and when.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use protocol::capability::Report;
use protocol::capability::output;
use protocol::event::{ContentBlock, EventBody};
use protocol::model::Seq;
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use tokio::sync::mpsc;

use super::super::Shared;
use super::lately::Lately;
use super::stand_in::{StandIn, Starting};
use crate::AgentOutput;

/// How long a busy agent has to answer before its line is said, and one that's only thinking about what was said,
/// whose answer usually comes by then, as the spec's cover-line rule says.
pub(super) const BUSY: Duration = Duration::from_millis(1_000);
pub(super) const THINKING: Duration = Duration::from_millis(2_500);

/// The newest prompt said out loud, which a line is said for if the agent hasn't answered it in time.
struct Waiting {
    prompt: Seq,
    /// The CLI has taken it, so the end of a turn is the end of the turn it was in. One sent while a turn runs can be
    /// taken only once that turn ends, and then its own turn starts.
    taken: bool,
    answered: bool,
}

#[derive(Default)]
struct State {
    lately: Lately,
    waiting: Option<Waiting>,
}

impl State {
    /// The agent has answered whatever it was waiting on, or has nothing more to answer.
    fn answered(&mut self) {
        if let Some(waiting) = &mut self.waiting {
            waiting.answered = true;
        }
    }
}

/// One agent's cover lines. Holds its output weakly, so the output ends when the agent's supervisor lets go of it,
/// whatever a line waiting to be said holds.
pub(in super::super) struct Covering {
    state: Mutex<State>,
    stand_in: tokio::sync::Mutex<StandIn>,
    output: mpsc::WeakSender<AgentOutput>,
    /// The agent has gone, so no line is said, and no spare is kept for one.
    stopped: AtomicBool,
}

impl Covering {
    pub(in super::super) fn new(starting: Starting, output: &mpsc::Sender<AgentOutput>) -> Self {
        Self {
            state: Mutex::new(State::default()),
            stand_in: tokio::sync::Mutex::new(StandIn::new(starting)),
            output: output.downgrade(),
            stopped: AtomicBool::new(false),
        }
    }

    /// The agent has gone, so its spare stops now rather than once it has waited long enough, and none starts again.
    pub(in super::super) fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        // One being asked stops its spare once it has its line.
        if let Ok(mut stand_in) = self.stand_in.try_lock() {
            stand_in.stop();
        }
    }

    /// Takes in what the agent said, as it's sent on.
    pub(in super::super) fn took_in(&self, output: &AgentOutput) {
        match output {
            AgentOutput::Event { body, .. } => self.heard(body),
            AgentOutput::Delta(delta) => self.writing(delta),
            AgentOutput::Retrying(_) | AgentOutput::Report(_) | AgentOutput::Exited { .. } => {}
        }
    }

    /// Takes in one of the agent's events.
    pub(super) fn heard(&self, body: &EventBody) {
        let mut state = self.state.lock();
        state.lately.heard(body);
        if let EventBody::PromptTaken { prompt } = body
            && let Some(waiting) = &mut state.waiting
            && waiting.prompt == *prompt
        {
            waiting.taken = true;
        }
        let wrote = matches!(body, EventBody::Assistant { subagent: None, blocks, .. }
            if blocks.iter().any(|block| matches!(block, ContentBlock::Text { .. })));
        let its_turn_ended =
            matches!(body, EventBody::TurnEnded { .. }) && state.waiting.as_ref().is_some_and(|waiting| waiting.taken);
        if wrote || its_turn_ended {
            state.answered();
        }
    }

    /// Takes in what the agent is writing.
    pub(super) fn writing(&self, delta: &StreamDelta) {
        let words =
            matches!(delta.change, DeltaChange::Text { .. } | DeltaChange::BlockStart { kind: BlockKind::Text, .. });
        if delta.subagent.is_none() && words {
            self.state.lock().answered();
        }
    }

    /// The prompt recorded at `prompt`, which says `text`, has been sent to the agent. One said out loud is covered for
    /// if the agent doesn't answer it in time.
    pub(in super::super) fn sent(shared: &Arc<Shared>, prompt: Seq, text: &str, spoken: bool) {
        if shared.covering.stopped.load(Ordering::SeqCst) {
            return;
        }
        let Some((asked, wait)) = shared.covering.asking(prompt, text, spoken) else { return };
        let shared = shared.clone();
        tokio::spawn(async move {
            let due = tokio::time::Instant::now() + wait;
            let line = {
                let mut stand_in = shared.covering.stand_in.lock().await;
                let line = stand_in.line(asked).await;
                // Stopped while it was asked, after it started the next spare.
                if shared.covering.stopped.load(Ordering::SeqCst) {
                    stand_in.stop();
                }
                line
            };
            #[expect(clippy::disallowed_methods, reason = "the agent's time to answer before its line is said")]
            tokio::time::sleep_until(due).await;
            let Some(line) = line else { return };
            if !shared.covering.unanswered(prompt) {
                return;
            }
            let Some(output) = shared.covering.output.upgrade() else { return };
            let said = Report::Covering(output::Covering { prompt, line });
            super::super::output::deliver(&shared, &output, AgentOutput::Report(said)).await;
        });
    }

    /// Notes that the prompt at `prompt`, which says `text`, was sent, and for one said out loud, says what to ask the
    /// stand-in, and how long the agent has to answer before the line is said. The newest said out loud is the one
    /// covered.
    pub(super) fn asking(&self, prompt: Seq, text: &str, spoken: bool) -> Option<(String, Duration)> {
        let mut state = self.state.lock();
        let asking = spoken.then(|| (state.lately.asking(text), if state.lately.in_turn { BUSY } else { THINKING }));
        state.lately.said_to(text);
        if asking.is_some() {
            state.waiting = Some(Waiting { prompt, taken: false, answered: false });
        }
        asking
    }

    /// Whether the agent still hasn't answered the prompt at `prompt`, which is the newest said out loud, and if so takes
    /// it as covered, so it's covered once.
    pub(super) fn unanswered(&self, prompt: Seq) -> bool {
        let mut state = self.state.lock();
        let unanswered = state.waiting.as_ref().is_some_and(|waiting| waiting.prompt == prompt && !waiting.answered);
        if unanswered {
            state.answered();
        }
        unanswered
    }
}
