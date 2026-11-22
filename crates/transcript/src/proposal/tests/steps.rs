//! A step on the page, as `Option-Up` and `Option-Down` take, and a press in its outline, which goes there the same
//! way.

use std::sync::Arc;

use gpui_kit::TestAppContext;

use super::super::ProposalView;
use super::support::{Hub, stand_in};
use crate::reveal::Wanted;
use crate::selection::Way;
use crate::step_keys::Steps as _;
use crate::texts::Of;

/// A step to a part of the page takes over from what was being brought into view, such as the end of what's selected
/// that a key moved, which would scroll back to it after. Pressing a part in the outline goes there the same way.
#[gpui_kit::test]
fn a_step_takes_over_from_what_was_being_brought_into_view(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        look::apply(cx);
    });
    let (client, _heard) = stand_in(Hub::Waits, cx);
    let mut proposal = protocol::fixtures::proposal("p");
    proposal.current.proposed.context = "Why the index matters.".into();
    let (view, cx) = cx.add_window_view(|_, cx| ProposalView::new(Arc::new(proposal), client, cx));
    cx.run_until_parked();
    view.update(cx, |view, _| view.selection.reveal().want(Wanted::End, Of::Document(0)));

    view.update(cx, |view, cx| view.step(Way::On, cx));

    let wanted = view.read_with(cx, |view, _| view.selection.reveal().wants_anything());
    assert!(!wanted, "it isn't brought into view after");
}
