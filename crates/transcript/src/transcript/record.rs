//! Following the session's record: whether the agent is working on a turn and since when, and what it's running in
//! the background, which the transcript's last row draws, and what the cache holds of the conversation, which the
//! newest turn's line and card say. Also its subagents' records, which their calls are drawn from, and in the subagent
//! tab, the one it shows, which says whether that one is at work.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{Context, Task};
use protocol::event::AgentTask;
use protocol::model::{PromptCache, Seq, Session, SessionStatus, Subagent, SubagentStatus};

use super::Transcript;

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
    /// What it's running in the background.
    pub(crate) background: Vec<AgentTask>,
    /// The event that ended the newest turn, whose line and card say how long the cache has left. 0 until one has.
    pub(crate) reply_seq: Seq,
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
            working: session.status == SessionStatus::Running,
            turn_started_at_ms: session.turn_started_at_ms,
            background: session.background_tasks.clone(),
            reply_seq: session.reply_seq,
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
            reply_seq: 0,
            cache: None,
        }
    }
}

impl Transcript {
    /// Follows the session's record.
    pub fn set_session(&mut self, session: &Session, cx: &mut Context<Self>) {
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
            m.at_work = at_work;
            replies
        };
        for reply in replies {
            self.redraw_reply(reply);
        }
        // The rest is drawn in the last row, with the streaming tail.
        let rows = self.list.row_count();
        self.list.out_of_date(rows.saturating_sub(1)..rows);
        cx.notify();
    }

    /// Draws the end of the turn `reply` again, for what it says of the cache. Not loaded, so not drawn either.
    fn redraw_reply(&self, reply: Seq) {
        if let Some(row) = self.session.read(|log| log.row_of(reply)) {
            self.list.out_of_date(row..row + 1);
        }
    }

    /// Follows the records of the session's subagents, and measures again each row that draws one that changed.
    pub fn set_subagents(&mut self, subagents: &[Subagent], cx: &mut Context<Self>) {
        let (rows, shown) = {
            let mut m = self.model.borrow_mut();
            let changed: Vec<&Subagent> = subagents
                .iter()
                .filter(|subagent| m.subagents.get(&subagent.call).map(Rc::as_ref) != Some(*subagent))
                .collect();
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
                m.subagents.insert(subagent.call.clone(), Rc::new(subagent.clone()));
            }
            (rows, shown.is_some())
        };
        for seq in rows {
            self.remeasure(seq, cx);
        }
        // The tab draws what its subagent was asked above its first row, and its report in its last. Its last words go
        // once its report says them, from whichever row they're in.
        if shown {
            let count = self.list.row_count();
            self.list.out_of_date(0..1);
            self.list.out_of_date(count.saturating_sub(1)..count);
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
