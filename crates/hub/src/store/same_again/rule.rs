//! The rule's table, a row for each write, and the test that goes through it.

use protocol::model::Rev;

use super::super::tests::store_with_session;
use super::super::{OneValue, Store, lock};
use super::{github, hub, proposals, sessions, workspaces};

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
const NONE_REPEATED: &[(&str, &str)] = &[(
    "github_apps",
    "an app's record is made once and removed, and what GitHub says of it is an observation, whose change changes \
     nothing on the record",
)];

const ROWS: &[Row] = &[
    row("repos", "its address, read", nothing, workspaces::read_address, workspaces::repo),
    row("workspaces", "its setup", nothing, workspaces::setup, workspaces::workspace),
    row("workspaces", "its branch", nothing, workspaces::branch, workspaces::workspace),
    row("diff_comments", "its text, edited", workspaces::commented, workspaces::edit, workspaces::comment),
    row("sessions", "its run's claims", nothing, sessions::claims, sessions::session),
    row("sessions", "read through", sessions::replied, sessions::read, sessions::session),
    row("sessions", "marked unread", sessions::replied_and_read, sessions::unread, sessions::session),
    row("sessions", "a draft", nothing, sessions::draft, sessions::session),
    row("sessions", "its draft cleared", sessions::drafted, sessions::no_draft, sessions::session),
    row("sessions", "a prompt queued", nothing, sessions::queue, sessions::session),
    row("sessions", "a prompt taken out", sessions::queued, sessions::unqueue, sessions::session),
    row("hub_settings", "keeping awake", nothing, hub::awake, hub::settings),
    row("hub_settings", "allowing what's asked", nothing, hub::allow, hub::settings),
    row("hub_settings", "what workers are sent", nothing, hub::sent, hub::settings),
    row("hub_settings", "the GitHub default", github::by_token, hub::no_default, hub::settings),
    row("hub_settings", "the providers GitHub is set up on", nothing, hub::providers, hub::settings),
    row("accounts", "its reserve", hub::ada, hub::reserve, hub::account),
    row("subagents", "its model", sessions::started, sessions::model, sessions::subagent),
    row("replies_heard", "how a reply ended, said again", nothing, sessions::reply_heard, sessions::reply_heard_rev),
    row("subagents", "what it holds", sessions::started, sessions::holds, sessions::subagent),
    row("new_workspace_draft", "a draft", nothing, hub::new_draft, hub::new_draft_rev),
    row("child_worktrees", "made ready", workspaces::made_child, workspaces::ready, workspaces::child),
    row("github_accounts", "its login cleared", github::by_token, github::clear, github::account),
    row("github_accounts", "signing in again", github::signed_in, github::needs, github::account),
    row("github_sign_ins", "where it's got to", github::waiting, github::refused, github::sign_in),
    row("proposals", "made again by its ask", nothing, proposals::propose, proposals::proposal),
    row("proposals", "answered again by its request", proposals::proposed, proposals::answer, proposals::proposal),
    row("proposals", "revised again by its ask", proposals::proposed, proposals::revise, proposals::proposal),
    row("proposals", "withdrawn again", proposals::proposed, proposals::withdraw, proposals::proposal),
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
    statement.query_map([], OneValue::read).unwrap().map(Result::unwrap).collect()
}

fn nothing(_: &Store) {}
