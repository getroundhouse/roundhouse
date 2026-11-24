//! What the hub keeps of an agent's background tasks.

use rusqlite::Transaction;

use crate::store::StoreError;

/// Each background task stored with no kind, given the kind `command`, which every task listed before the hub kept
/// kinds was, and which the hub read such a task as: in the lists of `agent_tasks` events, and in each chat's list of
/// what runs in the background. So a task always says its kind as it's stored. A session's events of the one kind are
/// found through the index of kinds, a session at a time, rather than by reading every event. Only a list with a task
/// that has no kind is written. What isn't JSON, or isn't a list of objects, is left as it is, as it was unreadable
/// before. Writing a chat's list moves its revision, as any change to it does.
pub(super) fn kinds_given(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let events = format!(
        "UPDATE events SET body = json_set(body, '$.tasks', json({given}))
             WHERE session_id IN (SELECT id FROM sessions) AND kind = 'agent_tasks' AND {lacking}",
        given = given("body, '$.tasks'"),
        lacking = lacking("body", "body, '$.tasks'"),
    );
    let sessions = format!(
        "UPDATE sessions SET background_tasks = {given} WHERE {lacking}",
        given = given("background_tasks"),
        lacking = lacking("background_tasks", "background_tasks"),
    );
    tx.execute_batch(&format!("{events};\n{sessions};"))?;
    Ok(())
}

/// The list of tasks at `list`, the arguments `json_each` takes, with each task that has no kind given `command`, in
/// its order, as text, since what a subquery gives loses its being JSON. Every task in it is an object, as
/// [`lacking`] has it.
fn given(list: &str) -> String {
    format!(
        "(SELECT json_group_array(CASE
                WHEN json_type(task.value, '$.kind') IS NULL THEN json_set(task.value, '$.kind', 'command')
                ELSE json(task.value)
            END ORDER BY task.key)
            FROM json_each({list}) AS task)"
    )
}

/// Whether `text` is JSON whose value at `list`, the arguments `json_each` takes, is a list of objects with one that
/// has no kind. It's a `CASE`, which SQLite reads in its order, where it may read the terms of an `AND` in any, so
/// nothing reads as JSON what isn't, which would fail the migration.
fn lacking(text: &str, list: &str) -> String {
    format!(
        "CASE
            WHEN json_valid({text}) IS NOT 1 THEN 0
            WHEN json_type({list}) IS NOT 'array' THEN 0
            WHEN EXISTS (SELECT 1 FROM json_each({list}) AS task WHERE task.type <> 'object') THEN 0
            ELSE EXISTS (SELECT 1 FROM json_each({list}) AS task WHERE json_type(task.value, '$.kind') IS NULL)
        END"
    )
}
