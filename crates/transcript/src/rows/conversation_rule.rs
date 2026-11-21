//! Where someone turned conversation mode on, to talk with the agent out loud: a rule across the chat, drawn as a
//! compaction's is, saying `Conversation mode` and when. It's all the chat shows of what the agent is told while the
//! mode is on, and turning it off draws nothing, since the next prompt, typed, has no microphone.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::{AnyElement, App, ParentElement, Role, StatefulInteractiveElement as _, TestSupportExt as _};
use look::IconSizing as _;
use protocol::model::Seq;

use super::{RULE_ICON, rule_middle, rule_row};
use crate::item_key::ItemKey;

/// The rule for the mode turned on at `seq`, at `at_ms`.
pub(super) fn line(seq: Seq, at_ms: i64, cx: &App) -> AnyElement {
    let said = |between: &str| said(look::day_and_time(at_ms, cx), between);
    let middle = rule_middle(ItemKey::Event(seq, 0).element("conversation-started"), cx)
        .role(Role::Label)
        .aria_label(said(", "))
        .child(Icon::new(IconName::AudioLines).sized(RULE_ICON))
        .child(said(" · "))
        .test_support();
    rule_row(middle)
}

/// What the rule says, with `between` before when, if that can be said.
fn said(when: Option<String>, between: &str) -> String {
    ["Conversation mode".to_string()].into_iter().chain(when).collect::<Vec<_>>().join(between)
}

#[cfg(test)]
mod tests {
    use super::said;

    #[test]
    fn the_rule_says_the_mode_and_when_it_was_turned_on() {
        assert_eq!(said(Some("2:14 PM".to_string()), " · "), "Conversation mode · 2:14 PM");
        assert_eq!(said(None, " · "), "Conversation mode", "a time too far off to say");
    }
}
