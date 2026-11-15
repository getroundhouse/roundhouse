//! What the command line says of why a turn started and how it failed: that the agent picked up by itself, and a
//! failed request, with how many times in a row it has failed.

use client_core::waits::{watched, why_it_ended};
use protocol::event::{Again, EndedTask, RequestFailure, TaskKind};

use super::printer::how_it_ended;

/// That the agent picked up by itself, and which of its tasks it heard had ended, each named as its list named it, but
/// for a wait, which says which pull requests it waited on and why it ended, from the hub's record, as the app does.
pub(super) fn own_turn(ended: &[EndedTask]) -> String {
    let each = ended.iter().map(|ended| {
        if ended.task.kind == TaskKind::Wait {
            let how = how_it_ended(ended.outcome);
            let (what, why) = match &ended.wait {
                Some(wait) => (watched(&wait.watching), wait.ended.map_or(how, |ended| why_it_ended(ended.how))),
                None => ("a pull request".to_string(), how),
            };
            return format!("waited on {what} ({why})");
        }
        let what = ended.task.description.as_deref().unwrap_or(ended.task.id.as_str());
        format!("{what} {}", how_it_ended(ended.outcome))
    });
    let each: Vec<String> = each.collect();
    if each.is_empty() {
        return "the agent picked up by itself".to_string();
    }
    format!("the agent picked up by itself, after: {}", each.join("; "))
}

/// A failed request, in the CLI's words, and if it repeats the one before, how many times in a row it has failed.
pub(super) fn request_failed(failure: &RequestFailure, again: Option<Again>) -> String {
    match again {
        Some(again) => format!("request failed again, {} times in a row: {}", again.times, failure.message),
        None => format!("request failed: {}", failure.message),
    }
}
