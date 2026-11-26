//! Following the session's record: whether the agent is working on a turn and since when, what it's running in the
//! background, and the wait it has open on the pull request, which the transcript's last row draws, and what the cache
//! holds of the conversation, which the newest turn's line and card say. Also its subagents' records, which their calls
//! are drawn from, and in the subagent tab, the one it shows, which says whether that one is at work, and what the hub
//! says of the chat's agent.

use std::sync::Arc;
use std::time::Duration;

use client_core::prompt_cache::{cache_worth_telling, idle_cache};
use client_core::records::{Snapshot, subagent_named};
use client_core::tool_servers::ToolServerNames;
use gpui_kit::{Context, SharedString, Task};
use protocol::event::AgentTask;
use protocol::model::{
    AgentDescription, PromptCache, Seq, Session, Subagent, SubagentId, SubagentStatus, Wait, WaitRead,
};

use super::{Transcript, TranscriptEvent};
use crate::rows::Opens;
use crate::task_lines;
use crate::texts::Of;

/// How often the time a turn has taken is drawn again while the agent works, for its tenths.
const TICK: Duration = Duration::from_millis(100);

/// What the agent is at, as the session's record says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AtWork {
    /// Working on a turn, which the transcript ends with the working line for. While the agent waits on someone, its
    /// card says so instead.
    pub(crate) working: bool,
    /// When the newest turn started, by the hub's clock. `None` until one has.
    pub(crate) turn_started_at_ms: Option<i64>,
    /// What it's running in the background, in the order it started them, but for what's part of the work of a subagent
    /// at work, which that subagent's tab shows. Its waits on the pull request are among them, and have no line, since
    /// `wait` is the hub's own of the one it has open.
    pub(crate) background: Vec<AgentTask>,
    /// The wait it has open on the pull request, as the hub keeps it, which the CLI's list of tasks only has a copy of,
    /// which can still list one that has ended.
    pub(crate) waits: Vec<OpenWait>,
    /// The event that ended the newest turn, whose line and card say how long the cache has left. 0 until one has.
    pub(crate) reply_seq: Seq,
    /// Whether that turn failed and no turn has started since, so the agent is stopped short on it, and the request it
    /// failed on can be tried again.
    pub(crate) failed: bool,
    /// What the cache holds of the conversation while it can expire, once the newest turn has ended. While the agent
    /// waits on the person mid-turn, the newest turn's line is an earlier turn's, and says nothing of it.
    pub(crate) cache: Option<IdleCache>,
}

/// What the cache holds of the conversation while it can expire, as [`client_core::prompt_cache::idle_cache`] says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdleCache {
    pub(crate) cache: PromptCache,
    /// Whether it's worth telling the person of beyond the turn's card, as
    /// [`client_core::prompt_cache::cache_worth_telling`] says.
    pub(crate) worth_telling: bool,
}

/// A wait the agent has open, and what it last read of the pull requests it watches, as
/// [`Snapshot::wait_read`] says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenWait {
    pub(crate) wait: Wait,
    pub(crate) read: Option<WaitRead>,
}

impl AtWork {
    /// What `session`'s record says, as `records`, the copy it's from, reads it.
    pub(crate) fn of(records: &Snapshot, session: &Session) -> Self {
        let cache = records.conversation_cache(session);
        let open = |wait: &Wait| OpenWait { wait: wait.clone(), read: records.wait_read(wait) };
        AtWork {
            working: session.status.is_working(),
            turn_started_at_ms: session.turn_started_at_ms,
            background: part_of(None, session),
            waits: session.waits.iter().map(open).collect(),
            reply_seq: session.reply_seq,
            failed: session.turn_failed(),
            cache: idle_cache(session, cache).filter(|_| !session.status.in_turn()).map(|held| IdleCache {
                cache: held.clone(),
                worth_telling: cache_worth_telling(session, cache).is_some(),
            }),
        }
    }

    /// A subagent at work, for the subagent tab, which ends with the working line while it is, then what's part of its
    /// work in the background, as `session`'s record says.
    fn of_subagent(subagent: &Subagent, session: Option<&Session>) -> Self {
        AtWork {
            working: subagent.status == SubagentStatus::Working,
            turn_started_at_ms: Some(subagent.started_at_ms),
            background: session.map_or_else(Vec::new, |session| part_of(Some(&subagent.id), session)),
            waits: Vec::new(),
            reply_seq: Seq::default(),
            failed: false,
            cache: None,
        }
    }
}

/// What `session` has in the background that's part of `subagent`'s work, or of no subagent's, as the chat's own is.
fn part_of(subagent: Option<&SubagentId>, session: &Session) -> Vec<AgentTask> {
    session.background_tasks.iter().filter(|task| task.part_of.as_ref() == subagent).cloned().collect()
}

impl Transcript {
    /// Takes what the hub says of the agent the chat runs, which a turn's card names, and what a run of its kind
    /// claims, which rows are drawn by, and draws every row again if either changed, which only a new connection does.
    pub fn set_agent(&mut self, agent: Option<&AgentDescription>, cx: &mut Context<Self>) {
        let named = agent.map(|agent| SharedString::from(agent.name.clone()));
        let claims = agent.map(|agent| agent.capabilities.clone());
        let changed = {
            let mut m = self.model.borrow_mut();
            let renamed = std::mem::replace(&mut m.agent, named) != m.agent;
            let claims_changed = std::mem::replace(&mut m.kind_claims, claims) != m.kind_claims;
            renamed || claims_changed
        };
        if changed {
            self.list.all_out_of_date();
            cx.notify();
        }
    }

    /// Takes what the client holds now of the chat's records, its session's, its subagents', its proposals' and how its
    /// replies read out loud ended, of the hub's tool servers, and what the hub says of the issues and pull requests its
    /// chips name. Each is taken only if it changed, so a client can call this whenever its records do. The client has
    /// all but the session's and the hub's only while it follows the chat's. The subagent tab's transcript takes
    /// whether it's at work from its subagent's record, not the chat's, and what that has in the background from the
    /// chat's.
    pub fn records_changed(&mut self, records: &Snapshot, cx: &mut Context<Self>) {
        self.caught_up_again(cx);
        let id = self.session_id.clone();
        let session = records.session(&id);
        if self.model.borrow().alone.is_none()
            && let Some(session) = session
        {
            self.set_session(records, session, cx);
        }
        self.set_subagents(records.subagents(&id), cx);
        self.follow_the_subagent(session, cx);
        self.set_proposals(records.proposals(&id), cx);
        self.set_replies_heard(records.replies_heard(&id), cx);
        self.set_tool_servers(&records.tool_server_names(&id), cx);
        self.references.records_changed(records, cx);
    }

    /// Takes what the person calls each of the hub's tool servers, and draws every row again if that changed, which only
    /// adding, renaming or removing one does, and the agent saying which it has.
    fn set_tool_servers(&mut self, names: &ToolServerNames, cx: &mut Context<Self>) {
        if self.model.borrow().tool_servers == *names {
            return;
        }
        self.model.borrow_mut().tool_servers.clone_from(names);
        // What's asked in the composer's place names the server its tool is from.
        self.tell_the_panel(cx);
        self.list.all_out_of_date();
        cx.notify();
    }

    /// Follows the session's record, as `records`, the copy it's from, reads it.
    pub fn set_session(&mut self, records: &Snapshot, session: &Session, cx: &mut Context<Self>) {
        // What the run claims, which rows are drawn by, changes as a run starts.
        if self.model.borrow().claims != session.agent_capabilities {
            self.model.borrow_mut().claims.clone_from(&session.agent_capabilities);
            self.list.all_out_of_date();
            cx.notify();
        }
        self.follow_approvals(session, cx);
        self.follow_question(cx);
        let at_work = AtWork::of(records, session);
        let replies = {
            let mut m = self.model.borrow_mut();
            if m.at_work == at_work {
                return;
            }
            if m.at_work.working != at_work.working || m.at_work.cache != at_work.cache {
                self.ticking = tick(&at_work, cx);
            }
            // The newest turn's line and card say what the cache holds, and an older one's say nothing of it.
            let replies = [m.at_work.reply_seq, at_work.reply_seq];
            task_lines::forget_gone(&at_work.background, &mut m.folds);
            m.at_work = at_work;
            replies
        };
        for reply in replies {
            self.redraw_reply(reply);
        }
        self.follow_retry();
        // The rest is drawn in the last row, with the streaming tail.
        self.redraw_end();
        cx.notify();
    }

    /// Measures the last row again, which draws the chat's end.
    pub(super) fn redraw_end(&self) {
        let rows = self.list.row_count();
        self.list.out_of_date(rows.saturating_sub(1)..rows);
    }

    /// Draws the end of the turn `reply` again, for what it says of the cache. Not loaded, so not drawn either.
    fn redraw_reply(&self, reply: Seq) {
        if let Some(row) = self.log.row_of(reply) {
            self.list.out_of_date(row..row + 1);
        }
    }

    /// Works out again which failed request's card offers Retry, and measures again the card that gains it and the one
    /// that loses it, since Retry makes a card taller. The record, the events and what's sent each change it, and the
    /// first two arrive apart in either order, so it's followed as each changes rather than as a card is drawn.
    pub(super) fn follow_retry(&self) {
        let now = crate::rows::retry_at(&self.log, &self.model.borrow().at_work);
        let before = std::mem::replace(&mut self.model.borrow_mut().retry_at, now);
        if before == now {
            return;
        }
        let rows: Vec<usize> = [before, now].into_iter().flatten().filter_map(|seq| self.log.row_of(seq)).collect();
        for row in rows {
            self.list.out_of_date(row..row + 1);
        }
    }

    /// Asks for the work of the subagent `opens` names in the subagent tab, named `title` until its record comes.
    pub(crate) fn open_subagent(&self, opens: &Opens, title: String, cx: &mut Context<Self>) {
        let subagent = match opens {
            Opens::Subagent(subagent) => subagent.clone(),
            Opens::StartedBy(call) => self.model.borrow().subagents.subagent_started_by(call),
        };
        cx.emit(TranscriptEvent::OpenSubagent { subagent, title });
    }

    /// Follows the records of the session's subagents, as client-core's snapshot shares them, and measures again each
    /// row that draws one that changed. A copy that's shared already is the one held.
    fn set_subagents(&mut self, subagents: &[Arc<Subagent>], cx: &mut Context<Self>) {
        let (rows, shown) = {
            let mut m = self.model.borrow_mut();
            // A tab opened on the subagent a call started, before its record came, names it by that call, and the hub
            // follows it by the ID its agent gives it once the agent says it started. Its record names it so.
            let named =
                m.alone.as_ref().and_then(|alone| subagent_named(subagents, alone)).map(|named| named.id.clone());
            if named.is_some() {
                m.alone = named;
            }
            let held = |subagent: &Arc<Subagent>| {
                m.subagents.get(&subagent.id).is_some_and(|held| Arc::ptr_eq(held, subagent))
            };
            let changed: Vec<&Arc<Subagent>> = subagents.iter().filter(|subagent| !held(subagent)).collect();
            if changed.is_empty() {
                return;
            }
            let drawn = m.subagent_rows.borrow();
            let rows: Vec<_> = changed.iter().filter_map(|subagent| drawn.get(&subagent.call).copied()).collect();
            drop(drawn);
            let shown = changed.iter().any(|subagent| m.alone.as_ref() == Some(&subagent.id));
            for subagent in changed {
                m.subagents.insert(Arc::clone(subagent));
            }
            (rows, shown)
        };
        for seq in rows {
            self.remeasure(seq, cx);
        }
        // What's asked in the composer's place names the subagent that asks, by its record.
        self.tell_the_panel(cx);
        // The tab draws what its subagent was asked above its first row, and its report in its last. Its last words go
        // once its report says them, from whichever row they're in.
        if shown {
            let tail = self.list.row_count().saturating_sub(1);
            for row in [Of::Head, Of::Foot].into_iter().filter_map(|of| crate::rows::list_row_of(of, tail)) {
                self.list.out_of_date(row..row + 1);
            }
            let repeated = {
                let m = self.model.borrow();
                crate::rows::rows_repeating_report(&self.log, &m)
            };
            for row in repeated {
                self.list.out_of_date(row..row + 1);
            }
        }
        cx.notify();
    }

    /// In the subagent tab, follows whether its subagent is at work, as that one's record says, and what it has in the
    /// background, as `session`'s does.
    fn follow_the_subagent(&mut self, session: Option<&Session>, cx: &mut Context<Self>) {
        {
            let mut m = self.model.borrow_mut();
            let Some(subagent) = m.alone.as_ref().and_then(|alone| m.subagents.get(alone)) else { return };
            let at_work = AtWork::of_subagent(subagent, session);
            if m.at_work == at_work {
                return;
            }
            if m.at_work.working != at_work.working {
                self.ticking = tick(&at_work, cx);
            }
            task_lines::forget_gone(&at_work.background, &mut m.folds);
            m.at_work = at_work;
        }
        self.redraw_end();
        cx.notify();
    }
}

/// Draws again what says how long something has taken or has left, as that changes. While the agent works, that's the
/// time so far, every [`TICK`]. A moving mark draws it on every frame anyway, and this keeps the time going when Reduce
/// Motion holds the mark still. Otherwise it's how long the cache has left, as each minute goes, until it has expired.
pub(super) fn tick(at_work: &AtWork, cx: &mut Context<Transcript>) -> Option<Task<()>> {
    if at_work.working {
        return Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
        }));
    }
    let (cache, reply) = (at_work.cache.as_ref()?.cache.clone(), at_work.reply_seq);
    let expires_at_ms = cache.expires_at_ms()?;
    Some(cx.spawn(async move |this, cx| {
        loop {
            let Ok(now_ms) = this.update(cx, |_, cx| look::now_ms(cx)) else { return };
            let Some(wait_ms) = look::next_minute_in_ms(expires_at_ms - now_ms) else { return };
            cx.background_executor().timer(Duration::from_millis(wait_ms.unsigned_abs())).await;
            let redrawn = this.update(cx, |transcript, cx| {
                transcript.redraw_reply(reply);
                cx.notify();
            });
            if redrawn.is_err() {
                return;
            }
        }
    }))
}
