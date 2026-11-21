//! What the agent asked in the screens' chats, once it's answered, when the transcript keeps it as a call's line,
//! whose questions and answers are copied with the rest of the chat while it's open.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use protocol::model::SessionStatus;
use spec_screens::question_chats::{MARK, MARKED, PR_STATE, REFRESH, REPLY, TWO_ASKED, TWO_MORE, answered};

use super::capture;
use super::worlds::chat_world;

/// Each question an opened call shows, and what was answered to it, is copied with the rest, in the order shown, each
/// a blank line apart. Their headers, and what says nobody answered, aren't the agent's words or the person's, so a
/// copy leaves them out.
#[test]
fn an_opened_call_s_questions_and_answers_are_copied_with_the_rest() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let world = || chat_world(SessionStatus::Idle, answered());

    let copied = capture::copied_whole(world, &[TWO_ASKED], &text_system);

    let shown = [TWO_MORE, MARK, MARKED, REFRESH, REPLY].join("\n\n");
    assert!(copied.contains(&shown), "{copied:?}");
    assert_eq!(copied.matches(PR_STATE).count(), 0, "and nothing of the call that isn't open: {copied:?}");
}
