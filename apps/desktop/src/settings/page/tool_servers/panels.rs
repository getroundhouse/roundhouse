//! The page's panels: the one a server is added or edited in, with what the way it knows the person takes, and the
//! sign-in's.

use client_core::tool_server_checks;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::super::field::{FIELDS_WIDTH, Field, captioned, labelled};
use super::super::panel::{self, Framed, button, buttons, waiting_line};
use super::Drawing;
use super::edits::Edits;
use super::form::{AuthFields, Form, Of, Way};
use super::going::FormStands;
use super::kept::{Kept, Panel, Shows};
use super::{sign_in_panel, words};

/// The page's name, which its panels' parts are known by.
const PAGE: &str = "mcp";

/// What the dropdown of the ways a server can know the person is called.
const AUTHENTICATION: &str = "Authentication";

/// How wide that dropdown is, and the header's field under it, so their ends line up.
const WAY_WIDTH: Pixels = px(200.);

/// How wide the dropdown's menu is, for a way and its note.
const WAYS_WIDTH: Pixels = px(320.);

/// `open`, whichever it is.
pub(super) fn draw(drawing: &Drawing<'_>, open: &Panel, cx: &App) -> AnyElement {
    let framed = match &open.shows {
        Shows::Form(form) => self::form(drawing, form, cx),
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

/// The panel a server is added or edited in: its name, its URL and how it knows the person, with the header and the key
/// it takes for a key, that the server is being asked for its tools while it is, and its buttons. A URL that's already
/// there says which server it is, and one that looks like it has a key in it says so, and neither stops anything.
fn form(drawing: &Drawing<'_>, form: &Form, cx: &App) -> Framed {
    let (url, edits, chosen) = (form.url.value(cx), form.edits(cx), form.auth.way());
    let had = match &form.of {
        Of::New(_) => None,
        Of::Had(was) => Some(&**was),
    };
    let there = drawing.records.tool_server_at(&url, had.map(|was| &was.id)).map(|server| &server.name);
    let key = match &form.auth {
        AuthFields::Key { header, key } => Some(key_row(header, key, edits, cx)),
        AuthFields::SignIn | AuthFields::Neither => None,
    };
    let asking = match &form.stands {
        FormStands::AskingServer { .. } => Some(self::asking(&url, cx)),
        FormStands::Open | FormStands::AskingHub { .. } | FormStands::Refused(_) | FormStands::NotListed(_) => None,
    };
    let way = h_flex().child(ways(drawing, chosen));
    let fields = v_flex()
        .gap(look::Space::Large)
        .child(labelled("Name", "mcp-name", &form.name.input, words::name_says(there), cx))
        .child(labelled("URL", "mcp-url", &form.url.input, words::url_says(&url, there), cx))
        .child(captioned(AUTHENTICATION, "mcp-way", way, words::way_says(chosen, edits), cx))
        .children(key)
        .children(asking);
    let (title, lead) = match had.zip(edits) {
        None => ("Add an MCP server".into(), words::ADD),
        Some((was, edits)) => {
            (words::edit_title(&was.name).into(), words::edit_says(edits, form.stands.saves_anyway()))
        }
    };
    Framed {
        title,
        lead: lead.into(),
        body: Some(fields.into_any_element()),
        refused: form.stands.why_not().map(str::to_string),
        buttons: form_buttons(drawing, form, cx),
    }
}

/// That the server at `url` is being asked for its tools, as assistive apps say it, and tests find it.
fn asking(url: &str, cx: &App) -> AnyElement {
    let said = SharedString::from(tool_server_checks::asking(url));
    let line = waiting_line(said.clone(), cx);
    line.id("mcp-asking").role(Role::Status).aria_label(said).test_support().into_any_element()
}

/// Cancel, and the form's main button, which is off while it can't be pressed. Once the server hasn't listed its tools,
/// that's Try again, with a button beside it that adds or saves it anyway.
fn form_buttons(drawing: &Drawing<'_>, form: &Form, cx: &App) -> Div {
    let off = !drawing.kept.read(cx).can_go(cx);
    let does = words::does(form.does(cx));
    let (anyway, goes) = match &form.stands {
        FormStands::NotListed(_) => {
            let anyway = button("mcp-anyway", words::anyway(does), false, off, drawing.act(Kept::go_anyway));
            (Some(anyway), words::TRY_AGAIN)
        }
        FormStands::Open | FormStands::AskingServer { .. } | FormStands::AskingHub { .. } | FormStands::Refused(_) => {
            (None, does)
        }
    };
    let cancel = button("mcp-cancel", "Cancel", false, false, drawing.act(Kept::put_away_panel));
    buttons().child(cancel).children(anyway).child(button("mcp-panel-go", goes, true, off, drawing.act(Kept::go)))
}

/// The dropdown of the ways a server can know the person, with `chosen` in it.
fn ways(drawing: &Drawing<'_>, chosen: Way) -> look::MenuButton {
    let kept = drawing.kept.clone();
    look::button("mcp-way", look::ButtonKind::Outlined, look::ControlSize::Regular, words::way_called(chosen))
        .accessibility_label(format!("{AUTHENTICATION}: {}", words::way_called(chosen)))
        .width(WAY_WIDTH)
        .menu(Anchor::TopLeft, move |menu: look::Menu, _, _| {
            Way::ALL.into_iter().fold(menu.width(WAYS_WIDTH), |menu, way| {
                let kept = kept.clone();
                let row = look::menu_row(words::way_called(way)).note(words::way_note(way), look::NoteKind::Detail);
                menu.row(row.chosen(way == chosen).on_choose(move |window, cx| {
                    kept.update(cx, |kept, cx| kept.choose(way, window, cx));
                }))
            })
        })
}

/// The header a key is sent in and the key, in a row as wide as the fields over it.
fn key_row(header: &Field, key: &Field, edits: Option<Edits>, cx: &App) -> Div {
    let header = labelled("Header", "mcp-header", &header.input, words::HEADER_SAYS, cx);
    let key = labelled("Key", "mcp-key", &key.input, words::key_says(edits), cx);
    h_flex()
        .items_start()
        .w_full()
        .max_w(FIELDS_WIDTH)
        .gap(look::Space::Large)
        .child(div().flex_none().w(WAY_WIDTH).child(header))
        .child(div().flex_1().min_w_0().child(key))
}
