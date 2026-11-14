//! A trial: one playing of a scenario, live or replayed, which can span several runs. Its checks read what the
//! adapter said, and its report says how it went.

use std::fmt;
use std::time::{Duration, Instant};

use protocol::capability::{Capabilities, laws};
use protocol::event::{CallKind, ContentBlock, EventBody, SessionEvent, TurnOutcome};
use protocol::model::{AgentSessionId, ModelName, Place, RunId, Seq};

use super::scenario::{Against, Failed, Scenario};
use crate::AgentOutput;

/// Something the adapter said, and when, in which of the trial's runs.
#[derive(Debug)]
pub struct Seen {
    pub run: RunId,
    pub at: Instant,
    pub output: AgentOutput,
}

/// Everything the adapter said in a trial, what a hub would have recorded of it, and when its steps marked what they
/// did.
#[derive(Debug, Default)]
pub struct Trial {
    pub seen: Vec<Seen>,
    /// What a hub would have recorded, numbered as it numbers its log.
    pub log: Vec<SessionEvent>,
    /// What each run claims.
    pub claims: Vec<(RunId, Capabilities)>,
    pub marks: Vec<(&'static str, Instant)>,
}

/// Records `body` at the end of `log`, under `run`, numbered as a hub numbers its log, and returns where.
pub(super) fn record(log: &mut Vec<SessionEvent>, run: Option<RunId>, body: EventBody) -> Seq {
    let (seq, place) = (Seq::new(log.len() as u64 + 1), Place::default());
    log.push(SessionEvent { session_id: "trial".into(), seq, ts_ms: 0, run, subagent: None, place, body, raw: None });
    seq
}

impl Trial {
    /// Whether the trial's log keeps the capability laws: that it settles each prompt, request, call and subagent
    /// once, records nothing of a run after it exited, and records nothing its run doesn't claim.
    pub fn laws(&self) -> Result<(), String> {
        let none = Capabilities::default();
        let claims = |run: RunId| self.claims.iter().find(|(of, _)| *of == run).map_or(&none, |(_, claims)| claims);
        let breaches = laws::check(&self.log, claims);
        if breaches.is_empty() {
            return Ok(());
        }
        let said = |breach: &laws::Breach| {
            let body = self.log.iter().find(|event| event.seq == breach.seq).map(|event| &event.body);
            format!("{:?} at {body:?}", breach.law)
        };
        Err(breaches.iter().map(said).collect::<Vec<_>>().join("; "))
    }

    /// Every event, of every run, in order.
    pub fn events(&self) -> impl Iterator<Item = &EventBody> {
        self.seen
            .iter()
            .filter_map(|seen| if let AgentOutput::Event { body, .. } = &seen.output { Some(body) } else { None })
    }

    /// Every block of the agent's own messages, and of what it added on the user's side, such as tool results.
    pub fn blocks(&self) -> impl Iterator<Item = &ContentBlock> {
        self.events().flat_map(|event| {
            if let EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } = event {
                blocks.as_slice()
            } else {
                &[]
            }
        })
    }

    /// Each call the adapter said is of `kind`.
    pub fn calls(&self, kind: &CallKind) -> Vec<&ContentBlock> {
        self.blocks()
            .filter(|block| match block {
                ContentBlock::ToolUse { call: Some(call), .. } => call == kind,
                ContentBlock::Call { call: protocol::capability::calls::Call::Command(_), .. } => {
                    *kind == CallKind::Command
                }
                ContentBlock::Text { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::Call { .. }
                | ContentBlock::CallResult { .. }
                | ContentBlock::ToolUse { .. }
                | ContentBlock::ToolResult { .. }
                | ContentBlock::Image { .. }
                | ContentBlock::Unknown => false,
            })
            .collect()
    }

    /// What each tool call gave back.
    pub fn results(&self) -> Vec<&str> {
        self.blocks()
            .filter_map(|block| match block {
                ContentBlock::CallResult {
                    output: protocol::capability::calls::CallOutput::Command(command), ..
                } => Some(command.output.as_str()),
                ContentBlock::ToolResult { text, .. }
                | ContentBlock::CallResult {
                    output: protocol::capability::calls::CallOutput::Words { text }, ..
                } => Some(text.as_str()),
                ContentBlock::Text { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::Call { .. }
                | ContentBlock::CallResult { .. }
                | ContentBlock::ToolUse { .. }
                | ContentBlock::Image { .. }
                | ContentBlock::Unknown => None,
            })
            .collect()
    }

    /// The conversation each run's agent said it had, as it started each turn, in order.
    pub fn conversations(&self) -> Vec<Option<&AgentSessionId>> {
        let inits = self.events().filter_map(|event| {
            if let EventBody::AgentInit { agent_session_id, .. } = event {
                Some(agent_session_id.as_ref())
            } else {
                None
            }
        });
        inits.collect()
    }

    /// What each turn said as it ended, in order.
    pub fn results_of_turns(&self) -> Vec<Option<&str>> {
        let ends = self.events().filter_map(|event| {
            if let EventBody::TurnEnded { result, .. } = event { Some(result.as_deref()) } else { None }
        });
        ends.collect()
    }

    /// How long after the step marked `from` the step marked `to` came, if both did.
    pub fn between(&self, from: &str, to: &str) -> Option<Duration> {
        let at = |name: &str| self.marks.iter().find(|(marked, _)| *marked == name).map(|(_, at)| *at);
        Some(at(to)?.saturating_duration_since(at(from)?))
    }

    /// The models the agent's own replies came from, in every turn, in the order they first did.
    pub fn models(&self) -> Vec<&ModelName> {
        let mut models = Vec::new();
        for event in self.events() {
            if let EventBody::TurnEnded { models: used, .. } = event {
                for model in used {
                    if !models.contains(&model) {
                        models.push(model);
                    }
                }
            }
        }
        models
    }

    /// How each turn ended, in order.
    pub fn outcomes(&self) -> Vec<TurnOutcome> {
        self.events()
            .filter_map(|event| if let EventBody::TurnEnded { outcome, .. } = event { Some(*outcome) } else { None })
            .collect()
    }

    /// What the conversation cost, as the last turn said.
    pub fn cost_usd(&self) -> Option<f64> {
        self.events()
            .filter_map(|event| if let EventBody::TurnEnded { cost_usd, .. } = event { *cost_usd } else { None })
            .last()
    }
}

/// How a trial played its scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Played {
    /// On the real CLI, against a model.
    Live(Against),
    /// From a capture of a trial on the scripted model, through a stand-in for the CLI.
    Replayed,
}

impl fmt::Display for Played {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Played::Live(against) => write!(f, "{against}"),
            Played::Replayed => f.write_str("replayed"),
        }
    }
}

/// How a trial went.
pub struct Report {
    pub scenario: &'static str,
    pub played: Played,
    /// The step that couldn't go on, if one couldn't. The scenario's checks don't run then.
    pub failed_step: Option<String>,
    pub checks: Vec<(&'static str, Result<(), String>)>,
    pub seconds: f64,
    /// What the real model cost.
    pub cost_usd: Option<f64>,
}

impl Report {
    /// Runs the scenario's checks over `trial`, once its steps have ended in `steps`, then `also`, the checks its
    /// binding makes of every trial played as `played`.
    pub fn of(
        scenario: &Scenario,
        played: Played,
        trial: &Trial,
        steps: Result<(), Failed>,
        also: Vec<(&'static str, Result<(), String>)>,
        started: Instant,
    ) -> Report {
        let mut checks: Vec<_> = match &steps {
            Ok(()) => scenario.checks.iter().map(|check| (check.name, (check.check)(trial))).collect(),
            Err(_) => Vec::new(),
        };
        checks.extend(also);
        if steps.is_ok() {
            checks.push(("the log keeps the capability laws", trial.laws()));
        }
        Report {
            scenario: scenario.name,
            played,
            failed_step: steps.err().map(|failed| failed.0),
            checks,
            seconds: started.elapsed().as_secs_f64(),
            cost_usd: if played == Played::Live(Against::RealModel) { trial.cost_usd() } else { None },
        }
    }

    pub fn passed(&self) -> bool {
        self.failed_step.is_none() && self.checks.iter().all(|(_, result)| result.is_ok())
    }

    /// For a test: panics, saying what failed, unless it passed.
    pub fn assert_passed(&self) {
        assert!(self.passed(), "{self}");
    }

    /// For a test of the checks themselves: panics unless the check `name` failed, saying something with `saying`.
    pub fn assert_failed(&self, name: &str, saying: &str) {
        let failed = self
            .checks
            .iter()
            .any(|(check, result)| *check == name && result.as_ref().is_err_and(|why| why.contains(saying)));
        assert!(failed, "expected {name:?} to fail, saying {saying:?}:\n{self}");
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verdict = if self.passed() { "PASS" } else { "FAIL" };
        write!(f, "{verdict}  {:<22} {:<9} {:>6.1}s", self.scenario, self.played.to_string(), self.seconds)?;
        if let Some(cost) = self.cost_usd {
            write!(f, "  ${cost:.5}")?;
        }
        if let Some(step) = &self.failed_step {
            write!(f, "\n        x {step}")?;
        }
        for (name, result) in &self.checks {
            if let Err(why) = result {
                write!(f, "\n        x {name}: {why}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::Capabilities;
    use protocol::event::EventBody;
    use protocol::model::{PermissionMode, RunId, Seq};
    use protocol::prompt::Prompt;

    use super::Trial;

    /// A trial whose one run took its prompt `taken` times, then said it went into plan mode, which it claims to be
    /// able to do or not.
    fn trial(taken: usize, claims_modes: bool) -> Trial {
        let run = Some(RunId::new(1));
        let mut log = Vec::new();
        let mut record = |run, body| super::record(&mut log, run, body);
        record(run, EventBody::AgentStarted { pid: 1, resumed: false, build: None, previous_build: None });
        record(None, EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("Go") });
        for _ in 0..taken {
            record(run, EventBody::PromptTaken { prompt: Seq::new(2) });
        }
        record(run, EventBody::AgentMode { permission_mode: PermissionMode::Plan });
        let claims = if claims_modes {
            crate::codex::description().capabilities.unwrap_or_default()
        } else {
            Capabilities::default()
        };
        Trial { log, claims: vec![(RunId::new(1), claims)], ..Trial::default() }
    }

    #[test]
    fn a_log_that_keeps_the_laws_passes_and_one_that_breaks_them_says_how() {
        assert_eq!(trial(1, true).laws(), Ok(()));
        let twice = trial(2, true).laws().unwrap_err();
        assert!(twice.contains("PromptSettlesOnce"), "{twice}");
        let unclaimed = trial(1, false).laws().unwrap_err();
        assert!(unclaimed.contains("OnlyWhatsClaimed"), "{unclaimed}");
    }
}
