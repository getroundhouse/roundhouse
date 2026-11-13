//! Where an update has got to, as a card in the version's place.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, Stateful,
    Styled as _, TestSupportExt as _, Window, div, px,
};
use look::{Hand as _, Typeset as _};
use look::{IconSize, IconStyle};

use super::update_details::{Details, Shown};
use crate::app::{Agents, AppView, BuildLine, UpdateQuestion};

/// Where an update has got to, when there's something to say, as a card with the app's icon in the version's place,
/// and a mark in its corner that says which versions, and what happens next, on hover. Installing an update is the
/// card's main action, and when that would lose what's typed, the card asks first, as its title, in place of its
/// button. Finishing one happens by itself, and doing it now stops agents, so that's a quiet button under what it's
/// waiting on, and a dialog asks first.
pub(super) fn update_card(
    line: &BuildLine,
    question: Option<&UpdateQuestion>,
    cx: &mut Context<AppView>,
) -> Option<AnyElement> {
    let (about, action) = match line {
        BuildLine::UpdateReady { next, .. } => {
            ready_card(next, question.filter(|q| matches!(q, UpdateQuestion::Restart { .. })), cx)
        }
        BuildLine::Finishing { agents, .. } => (finishing_card(*agents, cx), None),
        BuildLine::Current { .. } | BuildLine::OtherHub { .. } => return None,
    };
    let details = Details::of(line)?;
    Some(UpdateCard { about, details, action }.into_any_element())
}

/// A component of its own, rather than a function, because it keeps whether its details show, and where it was last
/// drawn, which places them.
#[derive(IntoElement)]
struct UpdateCard {
    /// Beside the icon.
    about: Stateful<Div>,
    details: Details,
    /// Under the rest, across the card.
    action: Option<AnyElement>,
}

impl RenderOnce for UpdateCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let shown = window.use_keyed_state("update-details-shown", cx, |_, _| Shown::default());
        let details = self.details.shown(shown.read(cx), cx);
        let mark = Details::mark(&shown, cx);
        let colors = look::colors(cx);
        v_flex()
            .id("update-card")
            .w_full()
            .mt_2()
            .p_2p5()
            .gap_2p5()
            .rounded(px(10.))
            .bg(colors.raised)
            .border_1()
            .border_color(colors.edge)
            .child(
                h_flex()
                    .items_start()
                    .gap_2p5()
                    .child(look::icon(IconStyle::Working, IconSize::Px64, cx))
                    .child(self.about.flex_1())
                    .child(mark),
            )
            .when_some(self.action, ParentElement::child)
            .when_some(details, ParentElement::child)
            .on_prepaint(move |bounds, _, cx| shown.update(cx, |shown, _| shown.card_width = bounds.size.width))
            .test_support()
    }
}

/// What the card says beside the icon: its title, and a detail under it.
fn card_text(title: &str, detail: impl IntoElement, cx: &App) -> Stateful<Div> {
    v_flex()
        .id("update-about")
        .min_w_0()
        .child(div().typeset(look::Type::Body).emphasis().child(title.to_string()))
        .child(div().typeset(look::Type::Detail).text_color(look::colors(cx).text_muted).child(detail))
}

/// Where a version's changes are linked from: beside the version of an update that's waiting to be installed, which
/// is when someone is deciding to take it. The page it opens doesn't exist yet, so for now it opens nothing, as
/// `docs/spec/updates.md` says.
fn whats_new(cx: &App) -> Stateful<Div> {
    div()
        .id("whats-new")
        .typeset(look::Type::Detail)
        .text_color(look::colors(cx).link)
        .hand()
        .hover(gpui_kit::Styled::underline)
        .child("What's new")
}

/// An update is downloaded, and restarting installs it, unless the card is asking first.
fn ready_card(
    next: &str,
    question: Option<&UpdateQuestion>,
    cx: &mut Context<AppView>,
) -> (Stateful<Div>, Option<AnyElement>) {
    let title = question.map_or("Update ready", UpdateQuestion::title);
    let version = h_flex().gap_1().child(format!("Roundhouse {next} ·")).child(whats_new(cx));
    let about = card_text(title, version, cx);
    let action = if let Some(question) = question {
        asking(question, "Keep working", "Restart", AppView::restart_to_update_anyway, cx)
    } else {
        let restart = cx.listener(|t, _, _, cx| t.restart_to_update(cx));
        Button::new("restart-to-update")
            .hand()
            .primary()
            .small()
            .w_full()
            .label("Restart to update")
            .on_click(restart)
            .into_any_element()
    };
    (about, Some(action))
}

/// The app has updated, and the older hub is still busy, so it finishes by itself once it isn't, or now if asked.
fn finishing_card(agents: Agents, cx: &mut Context<AppView>) -> Stateful<Div> {
    // Under the text, and in line with it, past the button's own padding.
    let now = h_flex().pt_1().ml(px(-3.5)).child(restart_now(cx));
    card_text("Finishing update", waiting_on(agents), cx).child(now)
}

/// What the card asks before going ahead: what that loses, then a quiet button that leaves things as they are, and
/// the main one, which goes ahead.
fn asking(
    question: &UpdateQuestion,
    stay: &'static str,
    go: &'static str,
    go_ahead: fn(&mut AppView, &mut Context<AppView>),
    cx: &mut Context<AppView>,
) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    let stay = Button::new("update-not-now")
        .hand()
        .ghost()
        .small()
        .flex_1()
        .label(stay)
        .on_click(cx.listener(|t, _, _, cx| t.dismiss_update_question(cx)));
    let go = Button::new("update-go-ahead")
        .hand()
        .primary()
        .small()
        .flex_1()
        .label(go)
        .on_click(cx.listener(move |t, _, _, cx| go_ahead(t, cx)));
    v_flex()
        .id("update-question")
        .gap_2p5()
        .child(div().typeset(look::Type::Detail).text_color(muted).child(question.detail()))
        .child(h_flex().gap_2().child(stay).child(go))
        .test_support()
        .into_any_element()
}

/// What finishing an update is waiting on. An agent that's waiting on you is said apart, since the update waits on
/// you then, and reads as stuck otherwise: nothing in the sidebar shows as working.
fn waiting_on(agents: Agents) -> String {
    let working = match agents.working {
        0 => None,
        1 => Some("Waiting on 1 agent".to_string()),
        n => Some(format!("Waiting on {n} agents")),
    };
    match (working, agents.waiting_on_you) {
        (None, 0) => "Waiting on other work".to_string(),
        (Some(working), 0) => working,
        (None, 1) => "1 agent is waiting on you".to_string(),
        (None, n) => format!("{n} agents are waiting on you"),
        (Some(working), 1) => format!("{working}. 1 more is waiting on you."),
        (Some(working), n) => format!("{working}. {n} more are waiting on you."),
    }
}

/// A quiet button, since finishing an update now stops the agents it's waiting on, which a dialog says first.
fn restart_now(cx: &mut Context<AppView>) -> Button {
    let finish = cx.listener(|t, _, _, cx| t.offer_hub_restart(cx));
    Button::new("finish-update").hand().ghost().xsmall().label("Restart now").on_click(finish)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn says(working: usize, waiting_on_you: usize) -> String {
        waiting_on(Agents { working, waiting_on_you })
    }

    #[test]
    fn agents_at_work_are_what_it_waits_on() {
        assert_eq!(says(1, 0), "Waiting on 1 agent");
        assert_eq!(says(3, 0), "Waiting on 3 agents");
    }

    /// Such an agent never finishes by itself, so "Waiting on 1 agent" would have the update wait for good.
    #[test]
    fn an_agent_waiting_on_you_is_said_to_be() {
        assert_eq!(says(0, 1), "1 agent is waiting on you");
        assert_eq!(says(0, 2), "2 agents are waiting on you");
        assert_eq!(says(1, 1), "Waiting on 1 agent. 1 more is waiting on you.");
        assert_eq!(says(2, 3), "Waiting on 2 agents. 3 more are waiting on you.");
    }

    #[test]
    fn with_no_agents_it_waits_on_other_work() {
        assert_eq!(says(0, 0), "Waiting on other work");
    }
}
