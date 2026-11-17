//! The rule each write the hub repeats keeps: writing what a record holds already leaves its revision, and the hub's,
//! where they were. A client takes only a copy with a newer revision, so a revision that moved for nothing would send
//! every client a record it has, each time the hub hears the same again, as it does on every read of GitHub, git or an
//! agent's output.

use protocol::event::{ContentBlock, EventBody};
use protocol::model::{
    BranchName, Branches, ChildStatus, ChildWorktree, DraftToKeep, GhLogin, GithubAccountId, GithubAppId,
    GithubLoginKind, GithubRepositoryName, GithubUser, GithubUserId, HubSettingsChange, LimitWindow,
    NewWorkspaceDraftToKeep, QueuedPrompt, Reach, RepoUrl, Rev, SentCredential, Seq, SessionStatus, UsageLimit,
};
use protocol::prompt::Prompt;
use protocol::{client::github_apps::GithubManifestCode, fixtures};

use super::tests::{notice, store_with_session};
use super::{GithubAppMaking, GithubConnecting, Queued, StatusChange, Store, TurnEdge, lock};
use crate::login_files::LoginFile;

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
enum Said {
    /// Whether it changed the record.
    Changed(bool),
    /// The record, as the write gives it back whether or not it changed it, in its debug form.
    Record(String),
}

impl Said {
    fn record(record: &impl std::fmt::Debug) -> Self {
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
    row("repos", "its address, read", nothing, read_address, repo),
    row("workspaces", "another build", nothing, older_build, workspace),
    row("workspaces", "the hub's build", on_older_build, own_build, workspace),
    row("sessions", "read through", replied, read, session),
    row("sessions", "marked unread", replied_and_read, unread, session),
    row("sessions", "a draft", nothing, draft, session),
    row("sessions", "its draft cleared", drafted, no_draft, session),
    row("sessions", "a prompt queued", nothing, queue, session),
    row("sessions", "a prompt taken out", queued, unqueue, session),
    row("hub_settings", "keeping awake", nothing, awake, settings),
    row("hub_settings", "allowing what's asked", nothing, allow, settings),
    row("hub_settings", "what workers are sent", nothing, sent, settings),
    row("hub_settings", "the GitHub default", by_token, machines, settings),
    row("accounts", "its limits, heard", nothing, limits, account),
    row("subagents", "its model", started, model, subagent),
    row("subagents", "what it holds", started, holds, subagent),
    row("new_workspace_draft", "a draft", nothing, new_draft, new_draft_rev),
    row("child_worktrees", "its branches", made_child, branches, child),
    row("github_accounts", "its login cleared", by_token, clear, github),
    row("github_accounts", "signing in again", signed_in, needs, github),
    row("found_github_logins", "a login found", nothing, found, found_rev),
    row("github_apps", "its bot", made_app, bot, app),
    row("github_reach", "what it reaches", nothing, reach, reach_rev),
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

fn repo(store: &Store) -> Rev {
    store.get_repo(&"r".into()).unwrap().rev
}

fn read_address(store: &Store) -> Said {
    let url = RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap();
    Said::Changed(store.read_repo_url(&"r".into(), "/repo", Some(&url), None).unwrap().is_some())
}

fn workspace(store: &Store) -> Rev {
    store.get_workspace(&"w".into()).unwrap().rev
}

fn older_build(store: &Store) -> Said {
    Said::Changed(store.set_workspace_worker_build(&"w".into(), Some("0.9.0")).unwrap().is_some())
}

fn on_older_build(store: &Store) {
    older_build(store);
}

fn own_build(store: &Store) -> Said {
    Said::Changed(store.set_workspace_worker_build(&"w".into(), None).unwrap().is_some())
}

fn session(store: &Store) -> Rev {
    store.get_session(&"s".into()).unwrap().session.rev
}

/// A prompt, and the end of its turn.
fn replied(store: &Store) {
    let ends = StatusChange {
        status: SessionStatus::Idle,
        turn: TurnEdge::Ends(Some(protocol::event::TurnOutcome::Success)),
        background_tasks: None,
    };
    store.append(&"s".into(), None, &notice("prompt"), None, None, Some(SessionStatus::Running.into())).unwrap();
    store.append(&"s".into(), None, &notice("done"), None, None, Some(ends.into())).unwrap();
}

fn read(store: &Store) -> Said {
    Said::Changed(store.mark_read(&"s".into(), Seq::new(u64::MAX)).unwrap().changed)
}

fn replied_and_read(store: &Store) {
    replied(store);
    read(store);
}

fn unread(store: &Store) -> Said {
    Said::Changed(store.mark_unread(&"s".into()).unwrap().changed)
}

fn draft(store: &Store) -> Said {
    Said::Changed(store.set_draft(&"s".into(), &DraftToKeep::new("same")).unwrap().changed)
}

fn drafted(store: &Store) {
    draft(store);
}

fn no_draft(store: &Store) -> Said {
    Said::Changed(store.set_draft(&"s".into(), &DraftToKeep::default()).unwrap().changed)
}

fn queue(store: &Store) -> Said {
    let prompt = QueuedPrompt { client_msg_id: "a".into(), prompt: Prompt::new("first") };
    let Queued::Done(queuing) = store.queue_prompt(&"s".into(), &prompt).unwrap() else { panic!("refused") };
    Said::Changed(queuing.reading.changed)
}

fn queued(store: &Store) {
    queue(store);
}

fn unqueue(store: &Store) -> Said {
    Said::Changed(store.unqueue_prompt(&"s".into(), &"a".into()).unwrap().changed)
}

fn settings(store: &Store) -> Rev {
    store.hub_settings().unwrap().rev
}

fn awake(store: &Store) -> Said {
    let off = HubSettingsChange { keep_awake: Some(false), ..HubSettingsChange::default() };
    Said::record(&store.change_hub_settings(&off).unwrap())
}

fn allow(store: &Store) -> Said {
    let allowing = HubSettingsChange { allow_asked_in_full_access: Some(true), ..HubSettingsChange::default() };
    Said::record(&store.change_hub_settings(&allowing).unwrap())
}

fn sent(store: &Store) -> Said {
    Said::Changed(store.record_github_sent(&SentCredential::MachineOwn).unwrap().is_some())
}

fn machines(store: &Store) -> Said {
    Said::record(&store.set_github_default(None).unwrap())
}

fn account(store: &Store) -> Rev {
    store.account(&"ada".into()).unwrap().unwrap().rev
}

fn limits(store: &Store) -> Said {
    let limit = UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 4,
        resets_at_ms: Some(9_000),
        heard_at_ms: None,
    };
    Said::Changed(store.hear_limits(&"ada".into(), &[limit], false, 100).unwrap().is_some())
}

fn subagent(store: &Store) -> Rev {
    store.get_subagent(&"s".into(), &"agent".into()).unwrap().unwrap().rev
}

/// Whether recording `body` changed a subagent's record.
fn recorded(store: &Store, body: &EventBody) -> Said {
    Said::Changed(!store.append(&"s".into(), None, body, None, None, None).unwrap().subagents.is_empty())
}

fn started(store: &Store) {
    recorded(store, &fixtures::subagent_started("agent", false));
}

fn model(store: &Store) -> Said {
    let reply = EventBody::Assistant {
        message_id: None,
        subagent: Some("agent".into()),
        model: Some("claude-haiku-5-5".into()),
        blocks: vec![ContentBlock::Thinking { text: "x".into() }],
        first_index: None,
    };
    recorded(store, &reply)
}

fn holds(store: &Store) -> Said {
    recorded(store, &EventBody::SubagentContext { subagent: "agent".into(), tokens: 1_200 })
}

fn new_draft_rev(store: &Store) -> Rev {
    store.new_workspace_draft().unwrap().rev
}

fn new_draft(store: &Store) -> Said {
    let typed = NewWorkspaceDraftToKeep {
        repo_id: Some("r".into()),
        text: "fix the login".into(),
        ..NewWorkspaceDraftToKeep::default()
    };
    Said::Changed(store.set_new_workspace_draft(&typed).unwrap().1)
}

fn child(store: &Store) -> Rev {
    store.child_at(&"w".into(), "/c/x").unwrap().unwrap().rev
}

fn made_child(store: &Store) {
    let child = ChildWorktree {
        path: "/c/x".into(),
        branch: BranchName::from("alder/x"),
        created_at_ms: 1,
        status: ChildStatus::Creating,
        ..fixtures::child_worktree("a")
    };
    store.insert_child(&child, &"k1".into()).unwrap();
}

fn branches(store: &Store) -> Said {
    let on = Branches { head: protocol::model::Head::Branch { name: BranchName::from("alder/x") }, had: vec![] };
    Said::Changed(store.set_child_branches(&"w".into(), "/c/x", &on).unwrap().is_some())
}

fn github(store: &Store) -> Rev {
    store.github_account(&"g".into()).unwrap().unwrap().rev
}

/// The GitHub account `g`, ada's, connected `how`, which makes it the default.
fn connected(store: &Store, how: GithubLoginKind, ends_at_ms: Option<i64>) {
    let (account, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    let (connected_at_ms, installation, profile) = (100, None, None);
    let connecting = GithubConnecting {
        account: &account,
        user: &user,
        how,
        file: &file,
        connected_at_ms,
        ends_at_ms,
        installation,
        profile,
    };
    store.connect_github_login(&connecting).unwrap();
}

fn by_token(store: &Store) {
    connected(store, GithubLoginKind::Token, None);
}

fn signed_in(store: &Store) {
    connected(store, GithubLoginKind::SignIn, Some(9_000));
}

fn clear(store: &Store) -> Said {
    Said::Changed(store.clear_github_login(&"g".into()).unwrap().is_some())
}

fn needs(store: &Store) -> Said {
    Said::Changed(store.github_login_needs_sign_in(&"g".into()).unwrap().is_some())
}

fn ada() -> GithubUser {
    GithubUser { id: GithubUserId::new(7), name: "ada".into() }
}

fn found_rev(store: &Store) -> Rev {
    store.list_records().unwrap().found_github_logins[0].rev
}

fn found(store: &Store) -> Said {
    Said::record(&store.record_found_github_login(&"local".into(), &GhLogin::LoggedOut).unwrap())
}

const APP: GithubAppId = GithubAppId::new(9);

fn app(store: &Store) -> Rev {
    store.list_records().unwrap().github_apps[0].rev
}

fn made_app(store: &Store) {
    let (key, code, owner) = (LoginFile::new(), GithubManifestCode::new("a-code"), ada());
    let slug = "ada-roundhouse".into();
    let making =
        GithubAppMaking { id: APP, slug: &slug, name: "Ada's Roundhouse", owner: &owner, key: &key, made_from: &code };
    store.make_github_app(&making).unwrap();
}

fn bot(store: &Store) -> Said {
    let bot = GithubUser { id: GithubUserId::new(41), name: "ada-roundhouse[bot]".into() };
    Said::Changed(store.set_github_app_bot(APP, &bot).unwrap().is_some())
}

fn reach_rev(store: &Store) -> Rev {
    store.github_reach_of(&"r".into()).unwrap().unwrap().rev
}

fn reach(store: &Store) -> Said {
    let pushes = Reach::Pushes { repository: GithubRepositoryName::named("cli/cli"), username: "ada".into() };
    Said::record(&store.record_github_reach(&"r".into(), &pushes, Some(5)).unwrap())
}
