//! Checking a log against the laws, an event at a time.

use std::collections::HashSet;
use std::hash::Hash;

use super::needs;
use crate::capability::Capabilities;
use crate::event::{ContentBlock, EventBody, SessionEvent};
use crate::model::{ApprovalId, RunId, Seq, ToolUseId};

/// A contract the log can be checked against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Law {
    /// A prompt, or a compaction asked for, is taken or dropped only after it was recorded, by the hub once at most and
    /// before the agent, and by the agent once at most.
    PromptSettlesOnce,
    /// A request for approval is resolved only after it was made, by the hub once at most and before the agent, and by
    /// the agent once at most.
    ApprovalResolvesOnce,
    /// A call's result comes after the call, and a call has one result at most.
    ResultFollowsCall,
    /// A subagent ends only after it started, by the hub once at most and before the agent, and by the agent once at
    /// most.
    SubagentEndsOnce,
    /// Nothing is recorded of a run after it exited but its launch's start, which can come after a quick exit.
    NothingAfterExit,
    /// What the agent records needs only capabilities its run claims.
    OnlyWhatsClaimed,
}

/// A law the event at `seq` broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Breach {
    pub law: Law,
    pub seq: Seq,
}

/// Every law `log` breaks, in its order, where each run `run` claims `claims(run)`.
pub fn check<'c>(log: &[SessionEvent], claims: impl Fn(RunId) -> &'c Capabilities) -> Vec<Breach> {
    let mut ledger = Ledger::default();
    let mut breaches = Vec::new();
    for event in log {
        let broken = ledger.read(event, &claims);
        breaches.extend(broken.into_iter().map(|law| Breach { law, seq: event.seq }));
    }
    breaches
}

/// What the log has said so far, which the next event is read against.
#[derive(Default)]
struct Ledger {
    prompts: Settled<Seq>,
    requests: Settled<ApprovalId>,
    subagents: Settled<ToolUseId>,
    calls: HashSet<ToolUseId>,
    answered: HashSet<ToolUseId>,
    exited: HashSet<RunId>,
}

impl Ledger {
    fn read<'c>(&mut self, event: &SessionEvent, claims: &impl Fn(RunId) -> &'c Capabilities) -> Vec<Law> {
        let mut broken = Vec::new();
        let body = &event.body;
        let by_agent = event.run.is_some();
        if let Some(run) = event.run {
            if self.exited.contains(&run) && !matches!(body, EventBody::AgentStarted { .. }) {
                broken.push(Law::NothingAfterExit);
            }
            if !claims(run).has_all(needs(body)) {
                broken.push(Law::OnlyWhatsClaimed);
            }
        }
        if let EventBody::UserPrompt { .. } | EventBody::CompactionAsked { .. } = body {
            self.prompts.record(event.seq);
        }
        if let EventBody::PromptTaken { prompt } | EventBody::PromptDropped { prompt } = body
            && !self.prompts.settle(prompt, by_agent)
        {
            broken.push(Law::PromptSettlesOnce);
        }
        if let EventBody::ApprovalRequested { approval_id, .. } = body {
            self.requests.record(approval_id.clone());
        }
        if let EventBody::ApprovalResolved { approval_id, .. } = body
            && !self.requests.settle(approval_id, by_agent)
        {
            broken.push(Law::ApprovalResolvesOnce);
        }
        if let EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } = body
            && !self.calls_and_results(blocks)
        {
            broken.push(Law::ResultFollowsCall);
        }
        if let EventBody::SubagentStarted { call, .. } = body {
            self.subagents.record(call.clone());
        }
        if let EventBody::SubagentEnded { call, .. } = body
            && !self.subagents.settle(call, by_agent)
        {
            broken.push(Law::SubagentEndsOnce);
        }
        if let (EventBody::AgentExited { .. }, Some(run)) = (body, event.run) {
            self.exited.insert(run);
        }
        broken
    }

    /// Records the calls `blocks` make, and whether each result they hold answers a call made before it, once.
    fn calls_and_results(&mut self, blocks: &[ContentBlock]) -> bool {
        let mut kept = true;
        for block in blocks {
            if let ContentBlock::ToolUse { id, .. } = block {
                self.calls.insert(id.clone());
            }
            if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                kept &= self.calls.contains(tool_use_id) && self.answered.insert(tool_use_id.clone());
            }
        }
        kept
    }
}

/// Things of one kind that are each recorded, then settled: by the hub once at most and before the agent, and by the
/// agent once at most.
struct Settled<K> {
    recorded: HashSet<K>,
    by_hub: HashSet<K>,
    by_agent: HashSet<K>,
}

impl<K> Default for Settled<K> {
    fn default() -> Self {
        Settled { recorded: HashSet::new(), by_hub: HashSet::new(), by_agent: HashSet::new() }
    }
}

impl<K: Clone + Eq + Hash> Settled<K> {
    fn record(&mut self, key: K) {
        self.recorded.insert(key);
    }

    /// Whether settling `key` now, by the agent or by the hub, keeps the law.
    fn settle(&mut self, key: &K, by_agent: bool) -> bool {
        if !self.recorded.contains(key) {
            return false;
        }
        if by_agent {
            return self.by_agent.insert(key.clone());
        }
        !self.by_agent.contains(key) && self.by_hub.insert(key.clone())
    }
}
