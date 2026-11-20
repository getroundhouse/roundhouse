//! What the page keeps: what it says with no hub to ask, and which sign-in its panel shows.

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::fixtures::github_sign_in;
use protocol::model::{GithubAccountId, GithubSignInStatus};

use super::super::kept::{Asked, Kept};
use super::super::words;
use super::made_up::{ended, records};

#[gpui_kit::test]
fn with_no_hub_to_ask_making_a_default_or_disconnecting_says_so_where_it_was_asked(cx: &mut TestAppContext) {
    let kept = cx.new(|_| Kept::default());
    let (ada, bea) = (GithubAccountId::from("g1"), GithubAccountId::from("g2"));

    kept.update(cx, |kept, cx| kept.make_default(Some(ada.clone()), cx));
    kept.read_with(cx, |kept, _| {
        assert_eq!(kept.failed(&Asked::Account(ada.clone())), Some(words::NO_HUB));
        assert_eq!(kept.failed(&Asked::MachinesOwn), None);
    });
    kept.update(cx, |kept, cx| kept.make_default(None, cx));
    kept.read_with(cx, |kept, _| assert_eq!(kept.failed(&Asked::MachinesOwn), Some(words::NO_HUB)));
    kept.update(cx, |kept, cx| kept.disconnect(bea.clone(), cx));
    kept.read_with(cx, |kept, _| {
        assert_eq!(kept.failed(&Asked::Account(bea)), Some(words::NO_HUB), "under that account's row alone");
        assert_eq!(kept.failed(&Asked::Account(ada)), None);
        assert!(!kept.choosing());
    });
    kept.update(cx, |kept, cx| kept.sign_in(None, cx));
    kept.read_with(cx, |kept, _| assert!(kept.panel.is_none(), "a sign-in needs a hub to run it"));
}

/// Which sign-in the page's panel shows, if it shows one.
fn showing(kept: &Kept) -> Option<String> {
    kept.sign_in_panel().map(|panel| panel.id.to_string())
}

#[gpui_kit::test]
fn a_sign_in_that_waits_shows_wherever_git_does_unless_its_panel_was_put_away_here(cx: &mut TestAppContext) {
    let kept = cx.new(|_| Kept::default());
    let refused = ended("s0", GithubSignInStatus::Refused);
    let waiting = records(Vec::new(), vec![github_sign_in("s2"), github_sign_in("s1"), refused], None);

    kept.update(cx, |kept, cx| {
        kept.open_choosing(cx);
        kept.show_waiting(&waiting, cx);
    });
    kept.read_with(cx, |kept, _| assert_eq!(showing(kept).as_deref(), Some("s1"), "in place of Connect account's"));
    kept.update(cx, |kept, cx| kept.show_waiting(&records(Vec::new(), vec![github_sign_in("s0")], None), cx));
    kept.read_with(cx, |kept, _| assert_eq!(showing(kept).as_deref(), Some("s1"), "not in place of another's"));
    kept.update(cx, |kept, cx| {
        kept.panel = None;
        kept.put_away.insert("s1".into());
        kept.show_waiting(&waiting, cx);
    });
    kept.read_with(cx, |kept, _| assert_eq!(showing(kept).as_deref(), Some("s2")));
    kept.update(cx, |kept, cx| {
        kept.panel = None;
        kept.put_away.insert("s2".into());
        kept.show_waiting(&waiting, cx);
    });
    kept.read_with(cx, |kept, _| assert!(kept.panel.is_none(), "nor one that has ended"));
}
