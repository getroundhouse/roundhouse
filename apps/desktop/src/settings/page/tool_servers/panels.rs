//! The page's panels: Add server's, with the ways a server can know the person, the one for what adding it that way
//! takes, and the sign-in's.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, App, Entity, IntoElement, ParentElement as _, Styled as _, Window, div};

use super::super::field::labelled;
use super::super::panel::{self, Framed, button, buttons, choice, choices};
use super::Drawing;
use super::adding::{AddPanel, AuthFields};
use super::kept::{Kept, Panel, Shows};
use super::{sign_in_panel, words};

/// The page's name, which its panels' parts are known by.
const PAGE: &str = "mcp";

/// `open`, whichever it is.
pub(super) fn draw(drawing: &Drawing<'_>, open: &Panel, cx: &App) -> AnyElement {
    let framed = match &open.shows {
        Shows::Choosing => choosing(drawing, cx),
        Shows::Adding(adding) => add(drawing, adding, cx),
        Shows::SigningIn => match drawing.kept.read(cx).signing_in.read(cx).under_way() {
            Some(under_way) => sign_in_panel::sign_in(drawing, under_way, cx),
            // It has just gone, and the panel goes with it before the next frame.
            None => return div().into_any_element(),
        },
    };
    let (kept, scroll) = (drawing.kept.clone(), drawing.scroll.clone());
    let laid_out = move |bounds, window: &mut Window, cx: &mut App| {
        kept.update(cx, |kept, _| kept.laid_out(bounds, &scroll, window));
    };
    panel::framed(PAGE, &open.place, framed, laid_out, cx)
}

/// Add server, open: each way a server can know the person.
fn choosing(drawing: &Drawing<'_>, cx: &App) -> Framed {
    let off = !drawing.can_ask(cx);
    let (by_sign_in, with_key, with_neither) = (drawing.kept.clone(), drawing.kept.clone(), drawing.kept.clone());
    let sign_in = button("mcp-sign-in", "Sign in", false, off, move |window, cx| {
        by_sign_in.update(cx, |kept, cx| kept.open_sign_in(window, cx));
    });
    let key = button("mcp-use-key", "Paste a key", false, off, move |window, cx| {
        with_key.update(cx, |kept, cx| kept.open_key(window, cx));
    });
    let neither = button("mcp-use-neither", "Add it", false, off, move |window, cx| {
        with_neither.update(cx, |kept, cx| kept.open_neither(window, cx));
    });
    let says = "A header sent with every request, such as an API key. Only the hub keeps it.";
    let ways = [
        choice(Lucide::Globe, "Sign in to it", None, words::SIGN_IN_CHOICE, sign_in, cx),
        choice(Lucide::KeyRound, "Use a key", None, says, key, cx),
        choice(Lucide::Server, "Neither", None, words::NEITHER, neither, cx),
    ];
    Framed {
        title: "Add an MCP server".into(),
        lead: "How does it know who you are?".into(),
        body: Some(choices(ways, cx).into_any_element()),
        refused: None,
        buttons: buttons().child(cancel(drawing.kept)),
    }
}

/// The panel for adding a server: its URL and its name, the key and the header it's sent in, where it has one, and Add,
/// or Sign in, which is off until there's each, and while the hub hasn't answered. A URL that's already there says which
/// server it is, and stops nothing.
fn add(drawing: &Drawing<'_>, adding: &AddPanel, cx: &App) -> Framed {
    let there = drawing.records.tool_server_at(&adding.url.value(cx)).map(|server| &server.name);
    let fields = v_flex()
        .gap(look::Space::Large)
        .child(labelled("URL", "mcp-url", &adding.url.input, words::url_says(there), cx))
        .child(labelled("Name", "mcp-name", &adding.name.input, words::name_says(there), cx));
    let (title, lead, does, fields) = match &adding.auth {
        AuthFields::SignIn => ("Sign in to a server", words::SIGN_IN, "Sign in", fields),
        AuthFields::Key { header, key } => (
            "Use a key",
            "A header sent with every request agents make to the server.",
            "Add",
            fields
                .child(labelled("Header", "mcp-header", &header.input, "Most servers want Authorization.", cx))
                .child(labelled("Key", "mcp-key", &key.input, words::KEY_SAYS, cx)),
        ),
        AuthFields::Neither => ("Add an MCP server", words::NEITHER, "Add", fields),
    };
    let act = drawing.kept.clone();
    let add = button("mcp-panel-add", does, true, !drawing.kept.read(cx).can_add(cx), move |window, cx| {
        act.update(cx, |kept, cx| kept.add(window, cx));
    });
    Framed {
        title: title.into(),
        lead: lead.into(),
        body: Some(fields.into_any_element()),
        refused: adding.refused.clone(),
        buttons: buttons().child(cancel(drawing.kept)).child(add),
    }
}

/// Cancel, which puts the panel away, and any wait for the hub's answer with it.
fn cancel(kept: &Entity<Kept>) -> look::Button {
    let kept = kept.clone();
    button("mcp-cancel", "Cancel", false, false, move |window, cx| {
        kept.update(cx, |kept, cx| kept.put_away_panel(window, cx));
    })
}
