//! The rule's table, a row for each write, and the test that goes through it.

use protocol::model::Rev;

use super::super::tests::store_with_session;
use super::super::{Store, lock};
use super::{github, hub, sessions, workspaces};

/// A write the hub can be asked for again with nothing new in it.
struct Row {
    /// The table that holds the record.
    table: &'static str,
    /// What's written, to name the row when it fails.
    what: &'static str,
    /// Makes what the write is to, on a store that has the repository `r`, its workspace `w`, and the session `s`.
    given: fn(&Store),
    write: Writing,
    /// The record's revision.
    rev: Revision,
}

type Writing = fn(&Store) -> Said;
type Revision = fn(&Store) -> Rev;

const fn row(table: &'static str, what: &'static str, given: fn(&Store), write: Writing, rev: Revision) -> Row {
    Row { table, what, given, write, rev }
}

/// What a write gives back.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Said {
    /// Whether it changed the record.
    Changed(bool),
    /// The record, as the write gives it back whether or not it changed it, in its debug form.
    Record(String),
}

impl Said {
    pub(super) fn record(record: &impl std::fmt::Debug) -> Self {
        Self::Record(format!("{record:?}"))
    }

    /// What the same write should give back after this: that it changed nothing, or the record as it was.
    fn again(&self) -> Self {
        match self {
            Self::Changed(_) => Self::Changed(false),
            Self::Record(_) => self.clone(),
        }
    }
}

/// Tables whose records have a revision, and no write that leaves it when it holds what the record holds, with why.
const NONE_REPEATED: &[(&str, &str)] = &[
    ("diff_comments", "a comment is made and removed, and an edit is written whatever its text"),
    ("github_sign_ins", "a sign-in is started, and where it's got to is written whatever it was"),
];

const ROWS: &[Row] = &[
    row("repos", "its address, read", nothing, workspaces::read_address, workspaces::repo),
    row("workspaces", "another build", nothing, workspaces::older_build, workspaces::workspace),
    row("workspaces", "the hub's build", workspaces::on_older_build, workspaces::own_build, workspaces::workspace),
    row("sessions", "read through", sessions::replied, sessions::read, sessions::session),
    row("sessions", "marked unread", sessions::replied_and_read, sessions::unread, sessions::session),
    row("sessions", "a draft", nothing, sessions::draft, sessions::session),
    row("sessions", "its draft cleared", sessions::drafted, sessions::no_draft, sessions::session),
    row("sessions", "a prompt queued", nothing, sessions::queue, sessions::session),
    row("sessions", "a prompt taken out", sessions::queued, sessions::unqueue, sessions::session),
    row("hub_settings", "keeping awake", nothing, hub::awake, hub::settings),
    row("hub_settings", "allowing what's asked", nothing, hub::allow, hub::settings),
    row("hub_settings", "what workers are sent", nothing, hub::sent, hub::settings),
    row("hub_settings", "the GitHub default", github::by_token, hub::machines, hub::settings),
    row("accounts", "its limits, heard", nothing, hub::limits, hub::account),
    row("subagents", "its model", sessions::started, sessions::model, sessions::subagent),
    row("subagents", "what it holds", sessions::started, sessions::holds, sessions::subagent),
    row("new_workspace_draft", "a draft", nothing, hub::new_draft, hub::new_draft_rev),
    row("child_worktrees", "its branches", workspaces::made_child, workspaces::branches, workspaces::child),
    row("github_accounts", "its login cleared", github::by_token, github::clear, github::account),
    row("github_accounts", "signing in again", github::signed_in, github::needs, github::account),
    row("found_github_logins", "a login found", nothing, github::found, github::found_rev),
    row("github_apps", "its bot", github::made_app, github::bot, github::app),
    row("github_reach", "what it reaches", nothing, github::reach, github::reach_rev),
];

/// So a record with a revision can't be added without a row, since each table that has one is either among the rows
/// or says why it isn't.
#[test]
fn writing_what_a_record_holds_already_leaves_its_revision_and_the_hubs() {
    let dir = tempfile::tempdir().unwrap();
    let tables = with_revisions(&store_with_session(dir.path()));
    for table in &tables {
        let listed = ROWS.iter().any(|row| row.table == table) || NONE_REPEATED.iter().any(|(t, _)| t == table);
        assert!(listed, "{table} has a revision, and no row says what writing the same again does to it");
    }
    let named = ROWS.iter().map(|row| row.table).chain(NONE_REPEATED.iter().map(|(table, _)| *table));
    for table in named {
        assert!(tables.iter().any(|t| t == table), "{table} has no revision, so its row tests nothing");
    }

    for row in ROWS {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        (row.given)(&store);
        let before = store.list_records().unwrap().rev;

        let said = (row.write)(&store);
        let (first, listed) = ((row.rev)(&store), store.list_records().unwrap().rev);
        let said_again = (row.write)(&store);

        let name = format!("{}, {}", row.table, row.what);
        assert!(first > before, "{name}: the first write is a change, so it moves {before} on, but it's {first}");
        assert_ne!(said, Said::Changed(false), "{name}: the first write says it changed nothing");
        assert_eq!(said_again, said.again(), "{name}: the same again says it changed the record");
        assert_eq!((row.rev)(&store), first, "{name}: the same again moved the record's revision");
        assert_eq!(store.list_records().unwrap().rev, listed, "{name}: the same again moved the hub's revision");
    }
}

/// Every table with a `rev` column.
fn with_revisions(store: &Store) -> Vec<String> {
    let conn = lock(&store.reader);
    let select = "SELECT m.name FROM sqlite_master AS m JOIN pragma_table_info(m.name) AS c
                  WHERE m.type = 'table' AND c.name = 'rev'";
    let mut statement = conn.prepare(select).unwrap();
    statement.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect()
}

fn nothing(_: &Store) {}
