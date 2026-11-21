//! What a workspace's record keeps of its worktree and of GitHub, since the floor.

use rusqlite::{Transaction, params};
use serde_json::Value;

use crate::store::StoreError;

/// The repository source and worker environment settled when each workspace was created, as JSON. Kept in the hub's
/// store (not on the client-facing record) so later operations never query the provider's adapter or flip a workspace
/// between a checkout and a clone. The trigger names only the columns the record shows so neither internal column moves
/// `rev`. Every workspace created before this migration was made from its repository's checkout on the hub's machine.
pub(super) const PLACEMENT: &str = r#"
    ALTER TABLE workspaces ADD COLUMN repo_source TEXT;
    ALTER TABLE workspaces ADD COLUMN worker_environment TEXT;
    DROP TRIGGER workspaces_rev_on_update;
    CREATE TRIGGER workspaces_rev_on_update
        AFTER UPDATE OF repo_id, name, branch, base_ref, path, status, provider, instance_id, created_at_ms,
            archived_at_ms, setup, port_base, title, title_source, pull_request, user_active_at_ms, comment_draft,
            branches, pull_request_branch, worker_build
        ON workspaces
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE workspaces SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    UPDATE workspaces
        SET repo_source = json_object('kind', 'checkout', 'path', (SELECT path FROM repos WHERE repos.id = workspaces.repo_id)),
            worker_environment = '{"filesystem":"hub","clones":"persistent"}'
        WHERE repo_source IS NULL;
"#;

/// Each workspace's record keeps only which pull request is its own, as JSON, in `pull_request_link` in place of
/// `pull_request`, since what GitHub says of it is kept in the hub's cache of what it reads from GitHub. JSON's `null`
/// where the hub found none, and NULL until it has looked, as before. One whose address isn't a pull request's on
/// github.com is NULL, so the hub looks for it again. Every record it changes moves its revision, so a client that
/// keeps records hears it, and the workspaces' trigger takes the new column in the old one's place.
pub(super) fn pull_request_links(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let named = "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'workspaces_rev_on_update'";
    let revisions: String = tx.query_row(named, [], |row| row.get(0))?;
    tx.execute_batch(
        "DROP TRIGGER workspaces_rev_on_update;
         ALTER TABLE workspaces ADD COLUMN pull_request_link TEXT;",
    )?;
    tx.execute_batch(&revisions.replace(", pull_request,", ", pull_request_link,"))?;
    let rows = tx
        .prepare("SELECT id, pull_request FROM workspaces WHERE pull_request IS NOT NULL")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE workspaces SET pull_request_link = ?2 WHERE id = ?1")?;
    for (id, found) in rows {
        write.execute(params![id, link_of(&found)])?;
    }
    drop(write);
    tx.execute_batch("ALTER TABLE workspaces DROP COLUMN pull_request")?;
    Ok(())
}

/// The link to what `found`, a pull request the hub found as an older build kept it, or `null` for none, is: its
/// repository, number and address, and how many files were uncommitted when it was read, as the record keeps it. In
/// the order this build writes it, so the first read after the update that finds the same doesn't count as a change.
/// `None` for what isn't one.
fn link_of(found: &str) -> Option<String> {
    /// A link, as this build wrote one, whatever later builds do.
    #[derive(serde::Serialize)]
    struct Link<'a> {
        repository: &'a str,
        number: u64,
        url: &'a str,
        uncommitted: u64,
    }
    let found: Value = serde_json::from_str(found).ok()?;
    if found.is_null() {
        return Some("null".to_string());
    }
    let url = found.get("url")?.as_str()?;
    let number = found.get("number")?.as_u64()?;
    let (owner, name) =
        url.strip_prefix("https://github.com/")?.strip_suffix(&format!("/pull/{number}"))?.split_once('/')?;
    let repository = protocol::model::GithubRepositoryName::new(owner, name).ok()?;
    let uncommitted = found.pointer("/local/uncommitted").and_then(Value::as_u64).unwrap_or_default();
    serde_json::to_string(&Link { repository: repository.as_str(), number, url, uncommitted }).ok()
}
