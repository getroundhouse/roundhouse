//! What the hub's feed does to a screen's proposal tab, which a hub that holds one world from start to finish can't.

use std::sync::Arc;

use client_core::fake_hub::World;

use super::Driver;

impl Driver {
    /// Gives the proposal tab its proposal's record as `world` holds it, as the feed would once the agent revised it.
    pub(super) fn proposal_revised(&mut self, world: fn() -> World) {
        let records = world().proposals.into_values().flatten();
        self.update_pane(|pane, cx| {
            let view = pane.proposal_tab_view().unwrap();
            let shown = view.read(cx).proposal().id.clone();
            let revised = records.into_iter().find(|proposal| proposal.id == shown).unwrap();
            view.update(cx, |view, cx| view.set_proposal(Arc::new(revised), cx));
        });
    }
}
