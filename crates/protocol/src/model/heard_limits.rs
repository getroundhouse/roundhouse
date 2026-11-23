//! The rules for what's left of an account's usage limits, over the limits its agents last heard, which the hub places
//! chats by and every client says them by, so they agree.

use super::{LimitWindow, Reserve, UsageLimit};

/// What any agent under an account last heard of its usage limits, shortest window first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeardLimits<'a>(pub &'a [UsageLimit]);

impl<'a> HeardLimits<'a> {
    /// Its week, its [`LimitWindow::SevenDays`] limit, while that's so at `now_ms`: `None` until an agent has heard it,
    /// and once the week has started again since, by the clock of whoever asks.
    pub fn week(self, now_ms: i64) -> Option<&'a UsageLimit> {
        self.0
            .iter()
            .find(|limit| limit.window == LimitWindow::SevenDays)
            .filter(|week| week.resets_at_ms.is_none_or(|at_ms| at_ms > now_ms))
    }

    /// The limits that are used up at `now_ms`, as an agent last heard them, and haven't started again since, so
    /// nothing can run under the account until each has.
    pub fn used_up(self, now_ms: i64) -> impl Iterator<Item = &'a UsageLimit> {
        let current = move |limit: &&UsageLimit| limit.resets_at_ms.is_none_or(|at_ms| at_ms > now_ms);
        self.0.iter().filter(current).filter(|limit| limit.left_percent() == 0)
    }

    /// Whether what's left of the week at `now_ms` is down to `reserve`, what the account reserves of it, so a chat on
    /// it is spending what the person kept for themselves. Never, for an account that reserves none, or whose week
    /// nobody has heard.
    pub fn in_reserve(self, reserve: Option<Reserve>, now_ms: i64) -> bool {
        match (reserve, self.week(now_ms)) {
            (Some(reserve), Some(week)) => reserve.reached_at(week.left_percent()),
            _ => false,
        }
    }
}
