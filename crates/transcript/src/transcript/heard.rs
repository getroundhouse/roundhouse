//! Following how each of the agent's replies read out loud ended, as the hub records it, which dims what of each reply
//! wasn't heard, and says why on its line.

use std::sync::Arc;

use gpui_kit::Context;
use protocol::model::ReplyHeard;

use super::Transcript;

impl Transcript {
    /// Follows how each of the chat's replies read out loud ended, as client-core's snapshot shares them, and draws the
    /// chat again if any changed. A copy that's shared already is the one held. Neither what's dimmed nor the mark on
    /// a turn's line changes a row's height, so nothing's measured again.
    pub fn set_replies_heard(&mut self, heard: &[Arc<ReplyHeard>], cx: &mut Context<Self>) {
        {
            let mut m = self.model.borrow_mut();
            let held =
                |reply: &Arc<ReplyHeard>| m.heard.get(&reply.message).is_some_and(|held| Arc::ptr_eq(held, reply));
            if heard.len() == m.heard.len() && heard.iter().all(held) {
                return;
            }
            m.heard = heard.iter().map(|reply| (reply.message.clone(), Arc::clone(reply))).collect();
        }
        cx.notify();
    }
}
