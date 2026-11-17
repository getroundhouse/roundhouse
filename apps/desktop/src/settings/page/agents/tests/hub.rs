//! What the page keeps, against a stand-in hub: a hub that refuses to connect a login as too old to is taken for one.

use client_core::RecordsFeed;
use client_core::stand_in_hub::{self, Does};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::client::{ApiError, ClientRequest, ErrorCode, ReplyOutcome};
use protocol::model::AgentKind;

use super::super::super::stand_in_hub::{Blank, until};
use super::super::Asking;
use super::super::kept::{Kept, To};

/// Against a stand-in hub that says what makes a login, as `Kept` is drawn with, and refuses to connect one as too
/// old to, as a hub whose description runs ahead of what it can do would.
#[gpui_kit::test]
fn a_hub_that_refuses_to_connect_a_login_as_too_old_is_taken_for_one_from_then_on(cx: &mut TestAppContext) {
    // The hub answers on a real thread.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), |request| match request {
        ClientRequest::AccountConnect(_) => {
            Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "unknown request"))))
        }
        _ => Does::Nothing,
    });
    let asking = Asking { feed: RecordsFeed::start(&client), client };
    let window = cx.add_window(|_, _| Blank);
    let kept = cx.new(|_| Kept::default());
    window
        .update(cx, |_, window, cx| {
            let settings = cx.focus_handle();
            kept.update(cx, |kept, cx| {
                kept.drawn(Some(asking), settings, None, cx);
                kept.open_new(AgentKind::Claude, window, cx);
                let panel = kept.panel.as_ref().unwrap();
                let To::New { name } = &panel.to else { unreachable!("a new account's panel") };
                name.input.update(cx, |name, cx| name.set_value("Work", window, cx));
                panel.token.input.update(cx, |token, cx| token.set_value("a-made-up-token", window, cx));
                kept.connect(window, cx);
            });
        })
        .unwrap();

    until(cx, "the refusal", |cx| kept.read_with(cx, |kept, _| kept.refused_as_older));
    kept.read_with(cx, |kept, _| assert!(kept.panel.is_none(), "the panel goes, since nothing can be connected"));
}
