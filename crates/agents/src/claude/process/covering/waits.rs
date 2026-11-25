//! The waits a line is said in: one opened for each thing said out loud that the agent can't answer yet, until it
//! answers, as [`protocol::capability::output::CoverLine`] says.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::capability::Report;
use protocol::capability::output::{CoverLine, CoverWait};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{InteractionMode, Seq};
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use tokio::sync::mpsc;

use super::super::Shared;
use super::lately::Lately;
use super::stand_in::{StandIn, Starting};
use crate::AgentOutput;
use tracing::Instrument as _;

/// How long a busy agent has to answer before its line is said, and one that's only thinking about what was said,
/// whose answer usually comes by then, as the spec's cover-line rule says.
pub(super) const BUSY: Duration = Duration::from_millis(1_000);
pub(super) const THINKING: Duration = Duration::from_millis(2_500);

/// The wait open for the newest thing said out loud, until the agent answers it.
struct Wait {
    number: CoverWait,
    prompt: Seq,
    /// The CLI has taken it, so the end of a turn is the end of the turn it was in. One sent while a turn runs can be
    /// taken only once that turn ends, and then its own turn starts.
    taken: bool,
    /// A line has been said in it, so its closing is said too.
    said: bool,
}

#[derive(Default)]
struct State {
    lately: Lately,
    /// Whether the person talks with the agent out loud, as the hub last said.
    on: bool,
    /// The agent has gone, so nothing more is said, and no spare is kept.
    gone: bool,
    /// One wait at a time, for the newest thing said.
    open: Option<Wait>,
    /// How many waits have been opened, which numbers the next.
    opened: u32,
}

impl State {
    /// Closes the wait that's open, and says what to report of it: that it's over, if a line was said in it.
    fn close(&mut self) -> Option<Report> {
        let wait = self.open.take()?;
        wait.said.then_some(Report::Covered { wait: wait.number })
    }
}

/// One agent's cover lines.
pub(in super::super) struct Covering {
    state: Mutex<State>,
    /// Held while a line is checked and sent, and while each of the agent's output is taken in and sent on, so a line
    /// is sent before the words that answer what it covers, or not at all.
    pub(in super::super) saying: tokio::sync::Mutex<()>,
    stand_in: Mutex<StandIn>,
    /// Held weakly, so the output ends when the agent's supervisor lets go of it, whatever a line waiting holds.
    output: mpsc::WeakSender<AgentOutput>,
}

impl Covering {
    /// The cover lines of an agent launched with the person talking with it as `mode` says.
    pub(in super::super) fn new(starting: Starting, mode: InteractionMode, output: &mpsc::Sender<AgentOutput>) -> Self {
        let covering = Self {
            state: Mutex::new(State::default()),
            saying: tokio::sync::Mutex::new(()),
            stand_in: Mutex::new(StandIn::new(starting)),
            output: output.downgrade(),
        };
        covering.turned(mode);
        covering
    }

    /// The person talks with the agent as `mode` says from now on. While the mode is on, a spare is kept ready. Once
    /// it's off, nobody is listening, so the wait open is closed, which says so if a line was said in it.
    pub(in super::super) async fn mode(shared: &Shared, mode: InteractionMode) {
        if let Some(closed) = shared.covering.turned(mode) {
            shared.covering.say(shared, closed).await;
        }
    }

    /// Sends `report` on, under `saying`, unless nobody is reading.
    async fn say(&self, shared: &Shared, report: Report) {
        let _saying = self.saying.lock().await;
        if let Some(output) = self.output.upgrade() {
            super::super::output::send_on(shared, &output, AgentOutput::Report(report)).await;
        }
    }

    /// Notes `mode`, and returns the closing of the wait it ends, if a line was said in it.
    pub(super) fn turned(&self, mode: InteractionMode) -> Option<Report> {
        let on = mode == InteractionMode::Conversation;
        let closed = {
            let mut state = self.state.lock();
            if state.gone {
                return None;
            }
            state.on = on;
            if on { None } else { state.close() }
        };
        let mut stand_in = self.stand_in.lock();
        if on {
            stand_in.warm();
        } else {
            stand_in.stop();
        }
        closed
    }

    /// The agent has gone, so its spare stops, and nothing more is said.
    pub(in super::super) fn stop(&self) {
        let mut state = self.state.lock();
        state.gone = true;
        state.on = false;
        self.stand_in.lock().stop();
    }

    /// Takes in what the agent said, before it's sent on, and returns the closing of the wait it answers, if a line was
    /// said in it, to send after it. For a caller holding `saying`.
    pub(in super::super) fn took_in(&self, output: &AgentOutput) -> Option<Report> {
        let mut state = self.state.lock();
        let answers = match output {
            AgentOutput::Event { body, .. } => {
                state.lately.heard(body);
                if let EventBody::PromptTaken { prompt } = body
                    && let Some(wait) = &mut state.open
                    && wait.prompt == *prompt
                {
                    wait.taken = true;
                }
                answers(body, state.open.as_ref())
            }
            AgentOutput::Delta(delta) => writes_words(delta) && state.open.as_ref().is_some_and(|wait| wait.taken),
            AgentOutput::Retrying(_) | AgentOutput::Report(_) | AgentOutput::Exited { .. } => false,
        };
        if answers { state.close() } else { None }
    }

    /// The prompt recorded at `prompt`, which says `text`, has been sent to the agent. One said out loud while the mode
    /// is on opens a wait, in place of any open, and a line is said in it if the agent hasn't answered in time.
    pub(in super::super) fn sent(shared: &Arc<Shared>, prompt: Seq, text: &str, spoken: bool) {
        let Some((number, asked, wait, closed)) = shared.covering.opening(prompt, text, spoken) else { return };
        let shared = shared.clone();
        let span = shared.run.span();
        tokio::spawn(
            async move {
                let due = tokio::time::Instant::now() + wait;
                if let Some(closed) = closed {
                    shared.covering.say(&shared, closed).await;
                }
                let line = StandIn::line(&shared.covering.stand_in, asked).await;
                #[expect(clippy::disallowed_methods, reason = "the agent's time to answer before its line is said")]
                tokio::time::sleep_until(due).await;
                let Some(text) = line else { return };
                let _saying = shared.covering.saying.lock().await;
                if let Some(said) = shared.covering.unanswered(number, text)
                    && let Some(output) = shared.covering.output.upgrade()
                {
                    super::super::output::send_on(&shared, &output, AgentOutput::Report(said)).await;
                }
            }
            .instrument(span),
        );
    }

    /// Notes that the prompt at `prompt`, which says `text`, was sent, and for one said out loud while the mode is on,
    /// opens a wait for it, and says its number, what to ask the stand-in, how long the agent has to answer before the
    /// line is said, and the closing of the wait it replaced, if a line was said in that.
    pub(super) fn opening(
        &self,
        prompt: Seq,
        text: &str,
        spoken: bool,
    ) -> Option<(CoverWait, String, Duration, Option<Report>)> {
        let mut state = self.state.lock();
        let covered = spoken && state.on && !state.gone;
        let asking = covered.then(|| (state.lately.asking(text), if state.lately.in_turn { BUSY } else { THINKING }));
        state.lately.said_to(text);
        let (asked, wait) = asking?;
        let closed = state.close();
        state.opened += 1;
        let number = CoverWait(state.opened);
        state.open = Some(Wait { number, prompt, taken: false, said: false });
        Some((number, asked, wait, closed))
    }

    /// The line `text` to say in the wait `number`, if it's still open: the agent hasn't answered, and the mode hasn't
    /// gone off. It's the wait's one line. For a caller holding `saying`, which sends it before anything else.
    pub(super) fn unanswered(&self, number: CoverWait, text: String) -> Option<Report> {
        let mut state = self.state.lock();
        // Turning the mode off closes it, so one still open is in the mode.
        let wait = state.open.as_mut().filter(|wait| wait.number == number && !wait.said)?;
        wait.said = true;
        Some(Report::Covering(CoverLine { wait: number, number: 1, text }))
    }
}

/// Whether `body` ends the open wait, `open`: the agent has taken what it was opened for and written words of its own,
/// or ended that turn, or the CLI dropped it, as stopping the agent does. Words written before the agent took it are
/// of what it was busy with, since the CLI takes something said mid-turn only at its next request to the model.
/// Thinking, calls and a subagent's words don't end it either, since the agent is still busy.
fn answers(body: &EventBody, open: Option<&Wait>) -> bool {
    let Some(open) = open else { return false };
    let wrote = matches!(body, EventBody::Assistant { subagent: None, blocks, .. }
        if blocks.iter().any(|block| matches!(block, ContentBlock::Text { .. })));
    let dropped = matches!(body, EventBody::PromptDropped { prompt } if *prompt == open.prompt);
    open.taken && (wrote || matches!(body, EventBody::TurnEnded { .. })) || dropped
}

/// Whether `delta` is the agent starting to write words of its own.
fn writes_words(delta: &StreamDelta) -> bool {
    let words =
        matches!(delta.change, DeltaChange::Text { .. } | DeltaChange::BlockStart { kind: BlockKind::Text, .. });
    delta.subagent.is_none() && words
}
