//! Where a chat's agent started on another build than it last ran on, as after an update: a rule across the chat,
//! drawn as a compaction's is, saying which build it was on and which it's on now. It names Roundhouse and its worker,
//! which is what moved, so it isn't taken for the agent's CLI or its model updating. The hub records both builds as the
//! agent starts, which is before the agent takes the prompt that started it, so the rule is above that prompt, and
//! everything under it ran on the new build, however many updates came before.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::{
    AnyElement, App, ParentElement, Role, StatefulInteractiveElement as _, Styled, TestSupportExt as _, px,
};
use protocol::model::Seq;

use super::{RULE_ICON, rule_middle, rule_row};
use crate::item_key::ItemKey;

/// The rule for the agent's start at `seq`, on `build`, after its chat's agent last ran on `from`.
pub(super) fn line(seq: Seq, from: &str, build: &str, cx: &App) -> AnyElement {
    let said = format!("Roundhouse chat worker updated from {from} to {build}");
    let middle = rule_middle(ItemKey::Event(seq, 0).element("updated"), cx)
        .role(Role::Label)
        .aria_label(said.clone())
        .child(Icon::new(IconName::CircleFadingArrowUp).size(px(RULE_ICON)))
        .child(said)
        .test_support();
    rule_row(middle, cx)
}
