//! Following the session's record: whether the agent is working on a turn and since when, and what it's running in
//! the background, which the transcript's last row draws. Also its subagents' records, which their calls are drawn
//! from, and in the subagent tab, the one it shows, which says whether that one is at work.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{Context, Task};
use protocol::event::AgentTask;
use protocol::model::{Session, SessionStatus, Subagent, SubagentStatus};

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
}

impl AtWork {
    pub(crate) fn of(session: &Session) -> Self {
        AtWork {
            working: session.status == SessionStatus::Running,
            turn_started_at_ms: session.turn_started_at_ms,
            background: session.background_tasks.clone(),
        }
    }

    /// A subagent at work, for the subagent tab, which ends with the working line while it is.
    fn of_subagent(subagent: &Subagent) -> Self {
        AtWork {
            working: subagent.status == SubagentStatus::Working,
            turn_started_at_ms: Some(subagent.started_at_ms),
            background: Vec::new(),
        }
    }
}

impl Transcript {
    /// Follows the session's record.
    pub fn set_session(&mut self, session: &Session, cx: &mut Context<Self>) {
        let at_work = AtWork::of(session);
        {
            let mut m = self.model.borrow_mut();
            if m.at_work == at_work {
                return;
            }
            if m.at_work.working != at_work.working {
                self.ticking = at_work.working.then(|| tick(cx));
            }
            m.at_work = at_work;
        }
        // It's all drawn in the last row, with the streaming tail.
        let rows = self.list.row_count();
        self.list.out_of_date(rows.saturating_sub(1)..rows);
        cx.notify();
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
                    self.ticking = at_work.working.then(|| tick(cx));
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

/// Draws the transcript again every [`TICK`]. A moving mark draws it on every frame anyway, and this keeps the time
/// going when Reduce Motion holds the mark still.
pub(super) fn tick(cx: &mut Context<Transcript>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(TICK).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                return;
            }
        }
    })
}
