//! What agents have heard of each account's limits. A limit is the account's rather than a session's, so it's kept
//! by account, whichever session's agent heard it.

use protocol::model::{AccountId, LimitWindow, UsageLimit};
use rusqlite::params;

use super::{Store, StoreError, from_text, lock, text_of};

impl Store {
    /// Keeps what an agent under `account` heard at `heard_at_ms`. Each window is kept by itself, so hearing of one
    /// leaves what's known of another, and something heard earlier never replaces something heard later. A window
    /// this build has no name for isn't kept, since there'd be nothing to tell it from another by.
    pub fn hear_limits(&self, account: &AccountId, limits: &[UsageLimit], heard_at_ms: i64) -> Result<(), StoreError> {
        let conn = lock(&self.writer);
        for limit in limits.iter().filter(|limit| limit.window != LimitWindow::Unknown) {
            conn.execute(
                "INSERT INTO account_limits (account_id, window, used_percent, resets_at_ms, heard_at_ms)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT (account_id, window) DO UPDATE
                     SET used_percent = excluded.used_percent, resets_at_ms = excluded.resets_at_ms,
                         heard_at_ms = excluded.heard_at_ms
                     WHERE excluded.heard_at_ms >= heard_at_ms",
                params![account.as_str(), text_of(&limit.window), limit.used_percent, limit.resets_at_ms, heard_at_ms],
            )?;
        }
        Ok(())
    }

    /// What's been heard of `account`'s limits, shortest window first, each with when it was heard.
    pub fn account_limits(&self, account: &AccountId) -> Result<Vec<UsageLimit>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT window, used_percent, resets_at_ms, heard_at_ms FROM account_limits WHERE account_id = ?",
        )?;
        let rows = statement.query_map([account.as_str()], |row| {
            Ok(UsageLimit {
                window: from_text(row.get(0)?)?,
                used_percent: row.get(1)?,
                resets_at_ms: row.get(2)?,
                heard_at_ms: Some(row.get(3)?),
            })
        })?;
        let mut limits = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        limits.sort_by_key(|limit| limit.window);
        Ok(limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::open_in;

    fn limit(window: LimitWindow, used_percent: u8) -> UsageLimit {
        UsageLimit { window, used_percent, resets_at_ms: Some(9_000), heard_at_ms: None }
    }

    /// What's kept for the account, as window, how much is used, and when that was heard.
    fn kept(store: &Store, account: &str) -> Vec<(LimitWindow, u8, Option<i64>)> {
        let limits = store.account_limits(&account.into()).unwrap();
        limits.iter().map(|limit| (limit.window, limit.used_percent, limit.heard_at_ms)).collect()
    }

    #[test]
    fn each_accounts_limits_are_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_in(dir.path());

        store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap();
        store.hear_limits(&"grace".into(), &[limit(LimitWindow::SevenDays, 80)], 200).unwrap();

        assert_eq!(kept(&store, "ada"), [(LimitWindow::SevenDays, 4, Some(100))]);
        assert_eq!(kept(&store, "grace"), [(LimitWindow::SevenDays, 80, Some(200))]);
        assert_eq!(kept(&store, "nobody"), []);
    }

    #[test]
    fn hearing_of_one_window_leaves_the_other_and_the_shortest_comes_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_in(dir.path());
        let both = [limit(LimitWindow::SevenDays, 4), limit(LimitWindow::FiveHours, 60)];
        store.hear_limits(&"ada".into(), &both, 100).unwrap();

        store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 5)], 200).unwrap();

        assert_eq!(
            kept(&store, "ada"),
            [(LimitWindow::FiveHours, 60, Some(100)), (LimitWindow::SevenDays, 5, Some(200))]
        );
    }

    #[test]
    fn what_was_heard_earlier_never_replaces_what_was_heard_later() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_in(dir.path());
        store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 9)], 200).unwrap();

        store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap();

        assert_eq!(kept(&store, "ada"), [(LimitWindow::SevenDays, 9, Some(200))]);
    }

    #[test]
    fn a_window_with_no_name_here_isnt_kept() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_in(dir.path());

        store.hear_limits(&"ada".into(), &[limit(LimitWindow::Unknown, 4)], 100).unwrap();

        assert_eq!(kept(&store, "ada"), []);
    }

    #[test]
    fn limits_are_still_there_when_the_database_is_opened_again() {
        let dir = tempfile::tempdir().unwrap();
        store_and_drop(dir.path());

        assert_eq!(kept(&open_in(dir.path()), "ada"), [(LimitWindow::SevenDays, 4, Some(100))]);
    }

    fn store_and_drop(dir: &std::path::Path) {
        open_in(dir).hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap();
    }
}
