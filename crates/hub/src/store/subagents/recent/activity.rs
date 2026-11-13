use protocol::event::ContentBlock;
use protocol::model::{Call, CallResult, RecentCall, RecentWork, Said};
use protocol::short_text::start_of;

/// How many of its newest calls are kept, which is as many as its call shows.
pub(super) const CALLS_KEPT: usize = 3;
/// How much of what it writes between calls is kept, in characters: a long paragraph, more than its call shows of it.
pub(super) const TEXT_KEPT: usize = 1_000;
/// How much of what came back for a call is kept, in characters: all of a short one, such as most commands' and small
/// reads', which its call's line then counts, and the start of a long one, which says what went wrong if it failed. A
/// whole file read can be hundreds of kilobytes, and would go to every client with each of the subagent's next few
/// changes.
pub(super) const RESULT_KEPT: usize = 2_000;

/// Adds what one of its messages did, block by block, and says whether that changed anything. `wrote` says the message
/// is the subagent's own, so its text is what it wrote. Text in a `user` line was written to it, such as the prompt
/// the CLI sends a subagent run in the foreground, which its call already shows, so only what came back is taken
/// from one.
pub(in super::super) fn add(work: &mut RecentWork, blocks: &[ContentBlock], wrote: bool) -> bool {
    let mut changed = false;
    for block in blocks {
        match block {
            ContentBlock::Text { text, .. } if wrote && !text.trim().is_empty() => {
                // Once what it's written since is cut short, more of it changes nothing, so it sends no notice.
                if work.since.as_ref().is_some_and(|since| since.cut) {
                    continue;
                }
                let joined = match work.since.take() {
                    Some(since) => said(&format!("{}\n\n{text}", since.text)),
                    None => said(text),
                };
                work.since = Some(joined);
                changed = true;
            }
            ContentBlock::ToolUse { .. } => {
                let Some(call) = Call::of(block) else { continue };
                work.calls.push(RecentCall { before: work.since.take(), call, result: None });
                if work.calls.len() > CALLS_KEPT {
                    work.calls.remove(0);
                }
                changed = true;
            }
            // A tool the API runs itself, such as the advisor, sends its result in the subagent's own message, and
            // every other tool's comes in the next one from the user's side.
            ContentBlock::ToolResult { tool_use_id, is_error, text, images, encrypted, exit_code, denied, .. } => {
                let Some(made) = work.calls.iter_mut().find(|made| made.call.id == *tool_use_id) else { continue };
                let (kept, cut) = start_of(text, RESULT_KEPT);
                let (text, images, encrypted) = (kept.to_owned(), images.clone(), *encrypted);
                let (is_error, exit_code, denied) = (*is_error, *exit_code, denied.clone());
                made.result = Some(CallResult { is_error, text, cut, images, encrypted, exit_code, denied });
                changed = true;
            }
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => {}
        }
    }
    changed
}

fn said(text: &str) -> Said {
    let (text, cut) = start_of(text, TEXT_KEPT);
    Said { text: text.to_owned(), cut }
}
