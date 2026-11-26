//! The panel of the sign-in there is, as it stands: that the hub is finding out how, the page to approve it on, with a
//! button that opens it again, or why one to sign in again came to nothing, with what to do about that. Any other that
//! came to nothing is the panel it was started from again.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, Div, IntoElement as _, ParentElement as _, Styled as _};

use super::super::panel::{Framed, button, buttons, waiting_line};
use super::Drawing;
use super::kept::Kept;
use super::words;
use crate::tool_sign_in::{Stands, UnderWay};

pub(super) fn sign_in(drawing: &Drawing<'_>, under_way: &UnderWay, cx: &App) -> Framed {
    let (to, stands) = (under_way.to(), under_way.stands());
    let (lead, body) = match stands {
        Stands::Starting(_) => (words::sign_in(to).to_string(), Some(waits(words::starting(&to.host()), cx))),
        Stands::Waiting(waiting) => {
            let open = button("mcp-open-page", "Open it again", false, false, drawing.act(Kept::open_sign_in_page))
                .icon(Lucide::ExternalLink);
            let line = waiting_line(words::WAITING, cx);
            let body = v_flex().gap(look::Space::Large).child(h_flex().child(open)).child(line).into_any_element();
            (words::approve_on(waiting.host(), to), Some(body))
        }
        Stands::Finishing(_) => (words::sign_in(to).to_string(), Some(waits(words::FINISHING.to_string(), cx))),
        Stands::NoWay(no_page) => (no_page.why().to_string(), None),
        Stands::Failed(_) => (words::sign_in(to).to_string(), None),
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

/// Cancel while it's under way, and Close once it's not, with another try for one that came to nothing.
fn buttons_for(drawing: &Drawing<'_>, under_way: &UnderWay, cx: &App) -> Div {
    let off = !drawing.can_ask(cx);
    let put_away = |id, label| button(id, label, false, false, drawing.act(Kept::put_away_sign_in));
    let go_on = match under_way.stands() {
        Stands::Starting(_) | Stands::Waiting(_) | Stands::Finishing(_) => {
            return buttons().child(put_away("mcp-cancel", "Cancel"));
        }
        // It would come to the same, and the hub has the server already, which a key can't be given to.
        Stands::NoWay(_) => None,
        Stands::Failed(_) => {
            Some(button("mcp-try-again", words::TRY_AGAIN, true, off, drawing.act(Kept::try_sign_in_again)))
        }
    };
    buttons().child(put_away("mcp-close", "Close")).children(go_on)
}

fn waits(said: String, cx: &App) -> AnyElement {
    waiting_line(said, cx).into_any_element()
}
