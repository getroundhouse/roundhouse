//! Following the session's record: whether the agent is working on a turn and since when, and what it's running in
//! the background, which the transcript's last row draws.

use std::time::Duration;

use gpui_kit::{Context, Task};
use protocol::event::AgentTask;
use protocol::model::{Session, SessionStatus};

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
