//! A fake hub's chats, as a test makes them in its world: each with its log, and what the hub records of it from its
//! events, such as its title, its subagents and what's running in the background, and when its turns ran. And what a
//! hub reads of them back.

use std::collections::HashMap;

use protocol::client::{ApiError, ErrorCode};
use protocol::event::{ApprovalChange, EventBody, SessionEvent};
use protocol::fixtures::observed;
use protocol::model::{
    CacheTtl, ContextUsage, ConversationCache, ModelName, Observation, ObservationKey, Observed, OpenApproval, Place,
    PromptCache, ReplyHeard, Seq, Session, SessionId, SessionStatus, Subagent, SubagentId, Title, WorkspaceId,
};
use protocol::prompt::Prompt;

use super::world::{EVENT_GAP_MS, MINUTE_MS, NOW_MS, World};

impl World {
    /// The person did something in the chat at `at_ms`, such as making it or sending the prompt that started a turn,
    /// which the hub notes on its workspace's record, unless they've done something there since. A chat made on its
    /// own, for a test of what's in it, has no workspace here to note it on.
    fn worked_in(&mut self, chat: &SessionId, at_ms: i64) {
        let workspace = &self.sessions.iter().find(|s| &s.id == chat).unwrap().workspace_id;
        let Some(workspace) = self.workspaces.iter_mut().find(|w| &w.id == workspace) else { return };
        workspace.user_active_at_ms = workspace.user_active_at_ms.max(Some(at_ms));
    }

    /// A chat holding these events. Its title is the start of the first prompt, and what's running in the background
    /// is what the newest event to say so says, as the hub makes them.
    pub fn chat(&mut self, workspace: &WorkspaceId, status: SessionStatus, events: Vec<EventBody>) -> SessionId {
        let id = SessionId::from(format!("chat-{}", self.sessions.len() + 1));
        let created_at_ms = self.next_time();
        let title = events.iter().find_map(|body| {
            let EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } = body else { return None };
            Title::from_first_line(text)
        });
        let log = log_of(&id, created_at_ms, events);
        self.subagents.insert(id.clone(), Subagent::recorded_from(&log));
        let last_seq = log.last().map_or(Seq::default(), |event| event.seq);
        let background_tasks =
            log.iter().rev().find_map(|event| event.body.sets_background()).unwrap_or_default().to_vec();
        // With the settings the hub gives a chat that names none.
        self.sessions.push(Session {
            workspace_id: workspace.clone(),
            status,
            last_seq,
            created_at_ms,
            updated_at_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(last_seq.get()).unwrap(),
            title,
            background_tasks,
            ..protocol::fixtures::session(id.as_str())
        });
        self.logs.insert(id.clone(), log);
        self.worked_in(&id, created_at_ms);
        id
    }

    /// Has each chat's record say which approvals its agent waits on, as a hub's does: each request in its log that no
    /// answer after it has closed, oldest first. A record the world gave a list of its own keeps it.
    pub(super) fn record_open_approvals(&mut self) {
        for session in self.sessions.iter_mut().filter(|session| session.open_approvals.is_none()) {
            let mut open: Vec<OpenApproval> = Vec::new();
            for event in self.logs.get(&session.id).into_iter().flatten() {
                match event.body.approval_change() {
                    Some(ApprovalChange::Opens { approval_id, .. }) => {
                        if !open.iter().any(|waiting| waiting.approval_id == *approval_id) {
                            open.push(OpenApproval { seq: event.seq, approval_id: approval_id.clone() });
                        }
                    }
                    Some(ApprovalChange::Closes(approval_id)) => {
                        open.retain(|waiting| waiting.approval_id != *approval_id);
                    }
                    None => {}
                }
            }
            session.open_approvals = Some(open);
        }
    }

    /// Has the agent been working on the chat's turn for this long when the picture is taken.
    pub fn working_for(&mut self, id: &SessionId, so_far_ms: i64) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert_eq!(session.status, SessionStatus::Running, "only a running chat has a turn going");
        session.turn_started_at_ms = Some(NOW_MS - so_far_ms);
        self.worked_in(id, NOW_MS - so_far_ms);
    }

    /// Has the turn that ends a chat's log taken this long by the hub's clock, as the hub says on its end and on the
    /// chat's record.
    pub fn last_turn_took(&mut self, id: &SessionId, took_ms: i64) {
        let ended = self.logs.get_mut(id).and_then(|log| log.last_mut()).unwrap();
        let EventBody::TurnEnded { started_at_ms, .. } = &mut ended.body else { panic!("the log ends mid-turn") };
        *started_at_ms = Some(ended.ts_ms - took_ms);
        let started = *started_at_ms;
        self.sessions.iter_mut().find(|s| &s.id == id).unwrap().turn_started_at_ms = started;
        self.worked_in(id, started.unwrap());
    }

    /// Has the turn that ends the last chat's log run from 11:57 PM to 12:04 AM the night before the picture, by the
    /// hub's clock, as the hub says on its end and on the chat's record.
    #[must_use]
    pub fn last_turn_past_midnight(mut self) -> World {
        let midnight_ms = NOW_MS - 15 * 60 * MINUTE_MS;
        let started_ms = midnight_ms - 3 * MINUTE_MS;
        let session = self.sessions.last_mut().unwrap();
        session.turn_started_at_ms = Some(started_ms);
        let ended = self.logs.get_mut(&session.id).and_then(|log| log.last_mut()).unwrap();
        let EventBody::TurnEnded { started_at_ms, .. } = &mut ended.body else { panic!("the log ends mid-turn") };
        *started_at_ms = Some(started_ms);
        ended.ts_ms = midnight_ms + 4 * MINUTE_MS;
        let chat = session.id.clone();
        self.worked_in(&chat, started_ms);
        self
    }

    /// Has the turn a chat's log ends with end this many minutes before [`NOW_MS`], and three seconds after its agent's
    /// newest request was sent to Opus 5.5 with a 1h cache, as the chat's cache says, with its context holding 313,500
    /// tokens unless its agent has said otherwise. The turn takes as long as it did. It's the chat's newest reply, which
    /// has been read.
    pub fn cached(&mut self, id: &SessionId, minutes_before_now: i64) {
        let used_at_ms = NOW_MS - minutes_before_now * MINUTE_MS;
        let ended = self.logs.get_mut(id).and_then(|log| log.last_mut()).unwrap();
        let EventBody::TurnEnded { started_at_ms, .. } = &mut ended.body else { panic!("the log ends mid-turn") };
        let moved_ms = used_at_ms + 3_000 - ended.ts_ms;
        *started_at_ms = started_at_ms.map(|at| at + moved_ms);
        ended.ts_ms += moved_ms;
        let reply = ended.seq;
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        session.turn_started_at_ms = session.turn_started_at_ms.map(|at| at + moved_ms);
        (session.reply_seq, session.read_seq) = (reply, reply);
        let context = self.context_said(id).unwrap_or_else(|| {
            let context = ContextUsage {
                used_tokens: 313_500,
                max_tokens: 500_000,
                model_tokens: None,
                max_set_by: None,
                parts: Vec::new(),
            };
            self.say_context(id, context.clone());
            context
        });
        let model = Some(ModelName::named("claude-opus-5-5"));
        let held = PromptCache { used_at_ms, ttl: CacheTtl::OneHour, model, tokens: context.used_tokens };
        self.observe(observed::cache(id.clone(), ConversationCache::worked_out(Some(held), Some(&context))));
    }

    /// Has the chat's agent say how full its context is, as the hub keeps it, and what's worth telling of the chat's
    /// cache worked out from it again, as the hub works it out.
    pub fn say_context(&mut self, id: &SessionId, context: ContextUsage) {
        let held = match self.observed(&ObservationKey::SessionCache { session_id: id.clone() }) {
            Some(Observation::SessionCache {
                cache: Observed::AsOf { value, .. } | Observed::Current { value },
                ..
            }) => value.held.clone(),
            _ => None,
        };
        self.observe(observed::cache(id.clone(), ConversationCache::worked_out(held, Some(&context))));
        self.observe(observed::context(id.clone(), context));
    }

    /// How full the chat's agent's context is, as it last said.
    fn context_said(&self, id: &SessionId) -> Option<ContextUsage> {
        match self.observed(&ObservationKey::SessionContext { session_id: id.clone() }) {
            Some(Observation::SessionContext {
                context: Observed::AsOf { value, .. } | Observed::Current { value },
                ..
            }) => Some(value.clone()),
            _ => None,
        }
    }

    /// Has everything in a chat before its event `seq` happen `idle_ms` earlier, so it had been idle that long when
    /// that one came.
    pub fn idle_before(&mut self, id: &SessionId, seq: Seq, idle_ms: i64) {
        for event in self.logs.get_mut(id).unwrap().iter_mut().filter(|event| event.seq < seq) {
            event.ts_ms -= idle_ms;
            if let EventBody::TurnEnded { started_at_ms, .. } = &mut event.body {
                *started_at_ms = started_at_ms.map(|at| at - idle_ms);
            }
        }
        self.sessions.iter_mut().find(|s| &s.id == id).unwrap().created_at_ms -= idle_ms;
    }

    /// Makes the end of a chat's log its newest reply, which has been read.
    pub fn read_reply(&mut self, id: &SessionId) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert!(session.last_seq > Seq::default(), "a chat with nothing in it has no reply");
        (session.reply_seq, session.read_seq) = (session.last_seq, session.last_seq);
    }

    /// Makes the end of a chat's log a reply nobody has read.
    pub fn unread(&mut self, id: &SessionId) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert!(session.last_seq > Seq::default(), "a chat with nothing in it has no reply");
        (session.reply_seq, session.read_seq) = (session.last_seq, Seq::default());
    }

    /// Closes a chat, this many minutes before [`NOW_MS`].
    pub fn close(&mut self, id: &SessionId, minutes_before_now: i64) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        session.closed_at_ms = Some(NOW_MS - minutes_before_now * MINUTE_MS);
    }

    pub fn session(&self, id: &SessionId) -> Result<&Session, ApiError> {
        self.sessions.iter().find(|s| &s.id == id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such chat"))
    }

    /// How each of the replies read out loud in the chat `id` ended.
    pub fn replies_heard_of(&self, id: &SessionId) -> Result<Vec<ReplyHeard>, ApiError> {
        self.session(id)?;
        Ok(self.replies_heard.get(id).cloned().unwrap_or_default())
    }

    pub fn events_from(&self, id: &SessionId, from_seq: Seq) -> impl Iterator<Item = &SessionEvent> {
        self.logs.get(id).into_iter().flatten().filter(move |event| event.seq >= from_seq)
    }

    /// The agent's own events, or one subagent's, from `from`.
    pub fn placed_from<'a>(
        &'a self,
        id: &SessionId,
        subagent: Option<&'a SubagentId>,
        from: Place,
    ) -> impl Iterator<Item = &'a SessionEvent> {
        let theirs = move |event: &&SessionEvent| event.subagent.as_ref() == subagent && event.place >= from;
        self.logs.get(id).into_iter().flatten().filter(theirs)
    }

    pub fn subagents_of(&self, id: &SessionId) -> Vec<Subagent> {
        self.subagents.get(id).cloned().unwrap_or_default()
    }

    /// The subagent `named` names in the chat `id`, as [`Subagent::named`] finds it, or else `named` as it is, which
    /// reads no events, as a hub takes it.
    pub(super) fn subagent_named(&self, id: &SessionId, named: Option<&SubagentId>) -> Option<SubagentId> {
        let named = named?;
        let found = self.subagents.get(id).and_then(|subagents| Subagent::named(subagents, named));
        Some(found.map_or_else(|| named.clone(), |found| found.id.clone()))
    }
}

/// A chat's events, with `seq` from 1 and no gaps, and each its place among the events from the same agent or subagent.
fn log_of(id: &SessionId, created_at_ms: i64, events: Vec<EventBody>) -> Vec<SessionEvent> {
    let mut places: HashMap<Option<SubagentId>, u64> = HashMap::new();
    events
        .into_iter()
        .zip((1..).map(Seq::new))
        .map(|(body, seq)| {
            let subagent = body.subagent().cloned();
            let place = places.entry(subagent.clone()).or_default();
            *place += 1;
            SessionEvent {
                session_id: id.clone(),
                ts_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(seq.get()).unwrap(),
                subagent,
                place: Place::new(*place),
                ..protocol::fixtures::event(seq, body)
            }
        })
        .collect()
}
