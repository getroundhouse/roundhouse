//! The panel of the sign-in there is, as it stands: that the hub is finding out how, the page to approve it on, with a
//! button that opens it again, or why it came to nothing, with what to do about that.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, Context, Div, Entity, IntoElement as _, ParentElement as _, Styled as _, Window};

use super::super::panel::{Framed, button, buttons, waiting_line};
use super::Drawing;
use super::kept::Kept;
use super::words;
use crate::tool_sign_in::{SignInTo, Stands, UnderWay};

pub(super) fn sign_in(drawing: &Drawing<'_>, under_way: &UnderWay, cx: &App) -> Framed {
    let (to, stands) = (under_way.to(), under_way.stands());
    let (lead, body) = match stands {
        Stands::Starting(_) => (words::SIGN_IN.to_string(), Some(waits(words::starting(&to.host()), cx))),
        Stands::Waiting(waiting) => {
            let open =
                button("mcp-open-page", "Open it again", false, false, act(drawing.kept, Kept::open_sign_in_page))
                    .icon(Lucide::ExternalLink);
            let line = waiting_line(words::WAITING, cx);
            let body = v_flex().gap(look::Space::Large).child(h_flex().child(open)).child(line).into_any_element();
            (words::approve_on(waiting.host()), Some(body))
        }
        Stands::Finishing(_) => (words::SIGN_IN.to_string(), Some(waits(words::FINISHING.to_string(), cx))),
        Stands::NoWay(why) => (words::no_way(why, to), None),
        Stands::Failed(_) => (words::SIGN_IN.to_string(), None),
    };
    let refused = match stands {
        Stands::Failed(why) => Some(why.clone()),
        Stands::Starting(_) | Stands::Waiting(_) | Stands::Finishing(_) | Stands::NoWay(_) => None,
    };
    Framed {
        title: words::sign_in_title(&to.name()).into(),
        lead: lead.into(),
        body,
        refused,
        buttons: buttons_for(drawing, under_way, cx),
    }
}

/// Cancel while it's under way, and Close once it's not, with the way to go on there is: a key in its place for a new
/// server that can't be signed in to, and another try for one that came to nothing.
fn buttons_for(drawing: &Drawing<'_>, under_way: &UnderWay, cx: &App) -> Div {
    let off = !drawing.can_ask(cx);
    let put_away = |id, label| button(id, label, false, false, act(drawing.kept, Kept::put_away_sign_in));
    let go_on = match (under_way.stands(), under_way.to()) {
        (Stands::Starting(_) | Stands::Waiting(_) | Stands::Finishing(_), _) => {
            return buttons().child(put_away("mcp-cancel", "Cancel"));
        }
        (Stands::NoWay(_), SignInTo::New(_)) => {
            Some(button("mcp-key-instead", "Use a key", true, off, act(drawing.kept, Kept::use_a_key_instead)))
        }
        // It has a name and a URL already, which a key can't be given to.
        (Stands::NoWay(_), SignInTo::Again { .. }) => None,
        (Stands::Failed(_), _) => {
            Some(button("mcp-try-again", "Try again", true, off, act(drawing.kept, Kept::try_sign_in_again)))
        }
    };
    buttons().child(put_away("mcp-close", "Close")).children(go_on)
}

/// What a button does to the page's state.
fn act(
    kept: &Entity<Kept>,
    act: fn(&mut Kept, &mut Window, &mut Context<Kept>),
) -> impl Fn(&mut Window, &mut App) + 'static {
    let kept = kept.clone();
    move |window, cx| kept.update(cx, |kept, cx| act(kept, window, cx))
}

fn waits(said: String, cx: &App) -> AnyElement {
    waiting_line(said, cx).into_any_element()
}
