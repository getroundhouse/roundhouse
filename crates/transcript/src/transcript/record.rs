//! Following the session's record: whether the agent is working on a turn and since when, what it's running in the
//! background, and the wait it has open on the pull request, which the transcript's last row draws, and what the cache
//! holds of the conversation, which the newest turn's line and card say. Also its subagents' records, which their calls
//! are drawn from, and in the subagent tab, the one it shows, which says whether that one is at work, and what the hub
//! says of the chat's agent.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{Context, SharedString, Task};
use protocol::event::AgentTask;
use protocol::model::{AgentDescription, PromptCache, Seq, Session, Subagent, SubagentStatus, Wait};

use super::Transcript;
use crate::prose::Of;
use crate::task_lines;

/// How often the time a turn has taken is drawn again while the agent works, for its tenths.
const TICK: Duration = Duration::from_millis(100);

/// What the agent is at, as the session's record says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AtWork {
    /// Working on a turn, which the transcript ends with the working line for. While the agent waits on someone, its
    /// card says so instead.
    pub(crate) working: bool,
    /// When the newest turn started, by the hub's clock. `None` from a hub that doesn't say.
    pub(crate) turn_started_at_ms: Option<i64>,
    /// What it's running in the background, in the order it started them. Its waits on the pull request are among
    /// them, and have no line, since `wait` is the hub's own of the one it has open.
    pub(crate) background: Vec<AgentTask>,
    /// The wait it has open on the pull request, as the hub keeps it, which the CLI's list of tasks only has a copy of,
    /// which can still list one that has ended.
    pub(crate) wait: Option<Wait>,
    /// The event that ended the newest turn, whose line and card say how long the cache has left. 0 until one has.
    pub(crate) reply_seq: Seq,
    /// Whether that turn failed and no turn has started since, so the agent is stopped short on it, and the request it
    /// failed on can be tried again.
    pub(crate) failed: bool,
    /// What the cache holds of the conversation while it can expire, once the newest turn has ended. While the agent
    /// waits on the person mid-turn, the newest turn's line is an earlier turn's, and says nothing of it.
    pub(crate) cache: Option<IdleCache>,
}

/// What the cache holds of the conversation while it can expire, as [`Session::idle_cache`] says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdleCache {
    pub(crate) cache: PromptCache,
    /// Whether it's worth telling the person of beyond the turn's card, as [`Session::cache_worth_telling`] says.
    pub(crate) worth_telling: bool,
}

impl AtWork {
    pub(crate) fn of(session: &Session) -> Self {
        AtWork {
            working: session.status.is_working(),
            turn_started_at_ms: session.turn_started_at_ms,
            background: session.background_tasks.clone(),
            wait: session.wait.clone(),
            reply_seq: session.reply_seq,
            failed: session.turn_failed(),
            cache: session.idle_cache().filter(|_| !session.status.in_turn()).map(|cache| IdleCache {
                cache: cache.clone(),
                worth_telling: session.cache_worth_telling().is_some(),
            }),
        }
    }

    /// A subagent at work, for the subagent tab, which ends with the working line while it is.
    fn of_subagent(subagent: &Subagent) -> Self {
        AtWork {
            working: subagent.status == SubagentStatus::Working,
            turn_started_at_ms: Some(subagent.started_at_ms),
            background: Vec::new(),
            wait: None,
            reply_seq: 0,
            failed: false,
            cache: None,
        }
    }
}

impl Transcript {
    /// Takes what the hub says of the agent the chat runs, which a turn's card names, and draws every row again if that
    /// changed, which only a new connection does.
    pub fn set_agent(&mut self, agent: Option<&AgentDescription>, cx: &mut Context<Self>) {
        let named = agent.map(|agent| SharedString::from(agent.name.clone()));
        if std::mem::replace(&mut self.model.borrow_mut().agent, named.clone()) != named {
            let rows = self.list.row_count();
            self.list.out_of_date(0..rows);
            cx.notify();
        }
    }

    /// Follows the session's record.
    pub fn set_session(&mut self, session: &Session, cx: &mut Context<Self>) {
        self.follow_approvals(session, cx);
        let at_work = AtWork::of(session);
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
        if let Some(row) = self.session.read(|log| log.row_of(reply)) {
            self.list.out_of_date(row..row + 1);
        }
    }

    /// Works out again which failed request's card offers Retry, and measures again the card that gains it and the one
    /// that loses it, since Retry makes a card taller. The record, the events and what's sent each change it, and the
    /// first two arrive apart in either order, so it's followed as each changes rather than as a card is drawn.
    pub(super) fn follow_retry(&self) {
        let now = self.session.read(|log| crate::rows::retry_at(log, &self.model.borrow().at_work));
        let before = std::mem::replace(&mut self.model.borrow_mut().retry_at, now);
        if before == now {
            return;
        }
        let rows: Vec<usize> =
            self.session.read(|log| [before, now].into_iter().flatten().filter_map(|seq| log.row_of(seq)).collect());
        for row in rows {
            self.list.out_of_date(row..row + 1);
        }
    }

    /// Follows the records of the session's subagents, as client-core's snapshot shares them, and measures again each
    /// row that draws one that changed. A copy that's shared already is the one held.
    pub fn set_subagents(&mut self, subagents: &[Arc<Subagent>], cx: &mut Context<Self>) {
        let (rows, shown) = {
            let mut m = self.model.borrow_mut();
            let held = |subagent: &Arc<Subagent>| {
                m.subagents.get(&subagent.call).is_some_and(|held| Arc::ptr_eq(held, subagent))
            };
            let changed: Vec<&Arc<Subagent>> = subagents.iter().filter(|subagent| !held(subagent)).collect();
            if changed.is_empty() {
                return;
            }
            let drawn = m.subagent_rows.borrow();
            let rows: Vec<_> = changed.iter().filter_map(|subagent| drawn.get(&subagent.call).copied()).collect();
            drop(drawn);
            let shown = changed.iter().find(|subagent| m.alone.as_ref() == Some(&subagent.call)).copied();
            if let Some(subagent) = shown {
                let at_work = AtWork::of_subagent(subagent);
                if m.at_work.working != at_work.working {
                    self.ticking = tick(&at_work, cx);
                }
                m.at_work = at_work;
            }
            for subagent in changed {
                m.subagents.insert(subagent.call.clone(), Arc::clone(subagent));
            }
            (rows, shown.is_some())
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
                self.session.read(|log| crate::rows::rows_repeating_report(log, &m))
            };
            for row in repeated {
                self.list.out_of_date(row..row + 1);
            }
        }
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
