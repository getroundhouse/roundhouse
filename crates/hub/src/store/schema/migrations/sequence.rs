use super::super::migration::Migration::{self, Rust, Sql};
use super::{accounts, drafts, workspaces};

/// The schema a new database starts from: what release 0.1.870 left once its migrations had run, as many as
/// [`update_feed::FLOOR`] counts. Each table, index and trigger is as SQLite kept it then, so a database made from this
/// is the same as one that release left, which the store's tests check against one it left.
pub(in super::super) const AT_FLOOR: &str = include_str!("at_floor.sql");

/// Applied in order, after the floor's. `PRAGMA user_version` records how many have run, counting the floor's.
pub(in super::super) const MIGRATIONS: &[Migration] = &[
    // Each session's events of each kind in the order they were recorded, so reading one kind reads only its events,
    // in `event_kinds`.
    Sql(super::event_kinds::IN_ORDER),
    // The newest to-do list each chat's agent reported, which its record shows, in `todos`.
    Sql(super::todos::TODOS),
    // The proposals agents make, in `proposals`.
    Sql(super::proposals::PROPOSALS),
    // How much of each account's week the person keeps for themselves, in `accounts`.
    Sql(accounts::RESERVES),
    // Whether each kind's chats are put on accounts automatically, in `account_defaults`.
    Sql(super::account_defaults::AUTOMATIC),
    // Whether Automatic chose the account each chat is on, in `session_accounts`.
    Sql(super::session_accounts::PLACED_AUTOMATICALLY),
    // Whether each chat couldn't start since Automatic found no account to spare, in `session_accounts`.
    Sql(super::session_accounts::NEEDS_AN_ACCOUNT),
    // The images each revision of a proposal points to, in `proposals`.
    Sql(super::proposals::REVISION_IMAGES),
    // The repository source and worker environment settled at each workspace's creation, in `workspaces`.
    Sql(workspaces::PLACEMENT),
    // What changed in each revision of a proposal, and the ask that wrote it, in `proposal_revisions`.
    Sql(super::proposals::REVISIONS),
    // The agent's withdrawal of each proposal it withdrew, in `proposals`.
    Sql(super::proposals::WITHDRAWALS),
    // Which device has each chat's conversation, in conversation mode, in `conversations`.
    Sql(super::conversations::CONVERSATION),
    // The providers whose machines Roundhouse leaves GitHub to, in `hub_settings`.
    Sql(super::github_accounts::GITHUB_LEFT_TO_MACHINE),
    // The providers Roundhouse sets up GitHub on, and who `gh` and the SSH key on the hub's machine are, in
    // `hub_settings`, without the table of each provider's `gh` login.
    Sql(super::github_accounts::GITHUB_SET_UP),
    // A GitHub account's removal, and none of the accounts the person disconnected before, in `github_accounts`.
    Sql(super::github_accounts::GITHUB_ACCOUNT_REMOVAL),
    // How the person was talking with the agent as each event was recorded, in `conversations`.
    Sql(super::conversations::INTERACTION),
    // How each reply said out loud ended, in `conversations`.
    Sql(super::conversations::REPLIES_HEARD),
    // Who said how each reply ended, in `conversations`.
    Sql(super::conversations::SETTLED_BY),
    // Each workspace's record keeping only which pull request is its own, in `workspaces`.
    Rust(workspaces::pull_request_links),
    // When what's in each session's composer was typed, in `drafts`.
    Sql(drafts::DRAFT_TYPED_AT),
    // Each wait's watch keeping the pull requests it saw as their facets, in `waits`.
    Rust(super::waits::pull_requests_as_facets),
    // Each repository's link to its repository on a host, and why one isn't linked, in `repos`.
    Rust(super::repos::links),
    // A repository's checkout may be none, for one added by its link alone, in `repos`.
    Rust(super::repos::without_checkouts),
];
