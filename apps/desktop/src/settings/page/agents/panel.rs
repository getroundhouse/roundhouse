//! The panel for connecting a login: the command that makes a token, to copy and run in a terminal of the person's own,
//! a field to paste the token into, and for a new account one for its name. Roundhouse never runs the command, signs
//! anyone in, or reads a terminal. It's brought into sight as it opens, wherever it is on the page.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use look::{Shaped as _, Typeset as _};

use super::super::field::{FIELDS_WIDTH, labelled as field};
use super::Drawing;
use super::kept::{Panel, To};
use super::words;

pub(super) fn panel(drawing: &Drawing<'_>, panel: &Panel, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let agent = drawing.agents.of(panel.agent);
    let login = agent.and_then(|agent| agent.login.as_ref());
    let lasting = login.and_then(|login| login.lasts_ms).and_then(words::lasting);
    let said = Said::of(&panel.to, agent.map(|agent| agent.name.as_str()), lasting.as_deref());
    let token =
        field("Token", "accounts-token", &panel.token.input, "Paste what it prints. Only the hub keeps it.", cx);
    let name = match &panel.to {
        To::New { name } => Some(field(
            "Name",
            "accounts-name",
            &name.input,
            "So you can tell it from the others. A chat under it shows this name.",
            cx,
        )),
        To::Account { .. } => None,
    };
    // An alert, so assistive apps say it as it comes.
    let refused = panel.refused.clone().map(|refused| {
        let refused = SharedString::from(refused);
        let said = div().id("accounts-refused").role(Role::Alert).aria_label(refused.clone());
        said.typeset(look::Type::Body).text_color(colors.error).child(refused).test_support()
    });
    let (kept, scroll) = (drawing.kept.clone(), drawing.scroll.clone());
    let card = look::card(look::CardKind::InPlace, look::CardWidth::Fills)
        .roomy()
        .heading(look::card_heading(said.title))
        .under_heading(div().text_color(colors.text_muted).child(said.lead))
        .children(login.map(|login| command(&login.made_with, cx)))
        .child(token)
        .children(name)
        .children(refused)
        .child(buttons(drawing, said.act, cx));
    div()
        .id("accounts-panel")
        .track_focus(&panel.focus)
        .relative()
        .child(card)
        .on_prepaint(move |bounds, window, cx| kept.update(cx, |kept, _| kept.laid_out(bounds, &scroll, window)))
        .test_support()
        .into_any_element()
}

/// What a panel says of itself, and what its main button does.
struct Said {
    title: String,
    lead: String,
    act: &'static str,
}

impl Said {
    /// For a panel connecting a login to `to`, of the kind of agent `called`, whose token lasts as long as `lasting`
    /// says, where the hub says.
    fn of(to: &To, called: Option<&str>, lasting: Option<&str>) -> Said {
        let lasting = lasting.map_or_default(|lasting| format!(", which lasts {lasting}"));
        let run = "Run this in a terminal of your own, and sign in";
        match to {
            To::New { .. } => Said {
                title: "Connect an account".to_string(),
                lead: format!(
                    "{run} when it asks. It prints a token for {}{lasting}. Roundhouse never runs it, and never sees \
                     your sign-in.",
                    called.unwrap_or("the agent")
                ),
                act: "Connect",
            },
            To::Account { name, held: true, .. } => Said {
                title: format!("Renew {name}"),
                lead: format!(
                    "{run} to the same account when it asks. Its new token takes the old one's place, so {name} keeps \
                     its limits and its chats."
                ),
                act: "Renew",
            },
            To::Account { name, held: false, .. } => Said {
                title: format!("Connect {name} again"),
                lead: format!("{run} to the same account when it asks, so {name} keeps its limits and its chats."),
                act: "Connect",
            },
        }
    }
}

/// The command that makes a token, as the person would type it, in code's font on a code block's colour, with a button
/// that copies it.
fn command(made_with: &str, cx: &App) -> Div {
    let colors = look::colors(cx);
    h_flex()
        .w_full()
        .max_w(FIELDS_WIDTH)
        .pl(look::Space::Medium)
        .pr(look::Space::XSmall)
        .py(look::Space::XSmall)
        .gap(look::Space::Small)
        .corners(look::Corner::Control)
        .border(look::EDGE)
        .border_color(colors.edge)
        .bg(colors.inset)
        .typeset(look::Type::Code)
        .child(div().text_color(colors.text_faint).child("$"))
        .child(div().flex_1().min_w_0().truncate().child(made_with.to_string()))
        .child(look::copy_button("accounts-copy-command", made_with.to_string(), "Copy command", cx))
}

/// Cancel, then the panel's main action, `act`, which is off until there's what connecting takes, and while the hub
/// hasn't answered.
fn buttons(drawing: &Drawing<'_>, act: &'static str, cx: &App) -> Div {
    let kept = drawing.kept.read(cx);
    let (cancel, connect) = (drawing.kept.clone(), drawing.kept.clone());
    h_flex()
        .justify_end()
        .gap(look::Space::Small)
        .child(
            look::button("accounts-cancel", look::ButtonKind::Outlined, look::ControlSize::Regular, "Cancel")
                .on_click(move |_, window, cx| cancel.update(cx, |kept, cx| kept.cancel(window, cx))),
        )
        .child(
            look::button(
                "accounts-panel-connect",
                look::ButtonKind::Main,
                look::ControlSize::Regular,
                SharedString::from(act),
            )
            .off(!kept.can_connect(cx))
            .on_click(move |_, window, cx| connect.update(cx, |kept, cx| kept.connect(window, cx))),
        )
}
