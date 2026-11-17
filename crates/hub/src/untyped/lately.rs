//! What a subagent's record said it had done lately, typed.

use protocol::model::{Lately, LatelyCall, LatelyResult};

use super::calls;
use super::older::{RecentCall, RecentWork};
use super::outputs::{self, Answered, Came};

/// What a subagent's record said it had done lately, from `recent`, the JSON its record kept that in, typed as `lately`
/// keeps it: each call typed, and what came back for it read against it. `None` for work with no call in it, which
/// says nothing `lately` needs, and for what can't be read, which is nothing the hub wrote.
pub(crate) fn lately(recent: &str) -> Option<Lately> {
    let RecentWork { calls, since } = serde_json::from_str(recent).ok()?;
    if calls.is_empty() {
        return None;
    }
    Some(Lately { calls: calls.into_iter().map(typed).collect(), since })
}

fn typed(RecentCall { before, call, result }: RecentCall) -> LatelyCall {
    let typed = calls::call(&call.name, &call.input);
    let result = result.map(|result| {
        let came = Came {
            text: &result.text,
            is_error: result.is_error,
            exit_code: result.exit_code,
            encrypted: result.encrypted,
            change: None,
            // Records kept what came back from before the adapter said what kind of call each answered, too, whose
            // text is as the CLI sent it. A text the adapter had read has neither the CLI's line saying how a command
            // exited nor its tags, so reading it as sent changes nothing.
            answered: if result.exit_code.is_some() { Answered::Other } else { Answered::Unsaid },
        };
        LatelyResult {
            output: outputs::of(Some(&typed), &came),
            is_error: result.is_error,
            cut: result.cut,
            images: result.images,
            denied: result.denied,
        }
    });
    LatelyCall { before, id: call.id, call: typed, result }
}
