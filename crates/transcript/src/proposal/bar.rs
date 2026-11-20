//! The bar at a proposal's tab's foot, which `answering` keeps the state of, and the banner over its page: the note and
//! what's chosen, with Another round and Send, while it waits for the person, and otherwise what was said to it. While
//! a newer revision is held back, as `held` says, the bar stays, with Another round and Send off, and the banner says
//! why.

use gpui_kit::component::input::Textarea;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};
use look::{IconSizing as _, Shaped as _, Typeset as _};
use protocol::model::{Choice, RevisionStanding, Round};

use super::answering::Sending;
use super::view::{COLUMN, ProposalView};

impl ProposalView {
    /// The bar at the tab's foot: the note and what's chosen, with Another round and Send, or once it's answered, what
    /// was chosen, or that another round was asked for, with the note. While the note's dictated into, its live words
    /// show under what's typed, the fine line runs along the bar's bottom, as along the composer's, and the recording's
    /// controls take the place of what's chosen, Another round and Send. Once the agent's withdrawn it, nothing, since
    /// nothing more can be said to it, and the banner says why.
    pub(super) fn foot(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (inside, dictation) = match self.proposal.standing() {
            RevisionStanding::Waiting => {
                let (input, dictation) = self.note(window, cx);
                let live = dictation.read(cx).live_line(cx).map(|line| {
                    div()
                        .w_full()
                        .pt(look::Space::XSmall)
                        .child(line.px(gpui_kit::component::Size::default().input_px()))
                });
                let field = div()
                    .id("proposal-note")
                    .w_full()
                    .child(Textarea::new(&input).appearance(false))
                    .children(live)
                    .test_support();
                let row = match dictation_ui::Field::controls(&dictation, look::ControlSize::Regular, cx) {
                    Some(controls) => controls.into_any_element(),
                    None => self.chosen_row(cx),
                };
                let inside = v_flex().w_full().gap(look::Space::Small).child(field).child(row).into_any_element();
                (inside, Some(dictation))
            }
            RevisionStanding::Chose(choice) => (self.chose_bar(choice, cx), None),
            RevisionStanding::AnotherRound(round) => (round_bar(round, cx), None),
            RevisionStanding::Responded => {
                let inside =
                    div().child("Responded, in a way this version of Roundhouse can't show").into_any_element();
                (inside, None)
            }
            RevisionStanding::Withdrawn(_) => return None,
        };
        let colors = look::colors(cx);
        let bar = div()
            .id("proposal-answer")
            .relative()
            .w_full()
            .max_w(px(COLUMN))
            .p(look::Space::Medium)
            .corners(look::Corner::Card)
            .border_1()
            .border_color(colors.edge)
            .bg(colors.raised)
            .typeset(look::Type::Body)
            .child(inside)
            .children(dictation.as_ref().map(|field| field.read(cx).edge()))
            .test_support();
        let bar = match &dictation {
            Some(field) => dictation_ui::Field::keys(bar, field).into_any_element(),
            None => bar.into_any_element(),
        };
        div()
            .flex_none()
            .w_full()
            .px(look::Space::Large)
            .pb(look::Space::Large)
            .flex()
            .justify_center()
            .child(bar)
            .into_any_element()
            .into()
    }

    /// What's chosen for each question, by its letter and label, Another round, and Send.
    fn chosen_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let another =
            look::button("another-round", look::ButtonKind::Edged, look::ControlSize::Regular, "Another round")
                .icon(gpui_kit::assets::IconName::RotateCw)
                .tooltip("Ask for another revision without choosing. Say in the note what to change")
                .off(self.round_ready(cx).is_none())
                .on_click(cx.listener(|this, _, _, cx| this.another_round(cx)));
        let send = Self::send_button(self.ready().is_none(), cx);
        let colors = look::colors(cx);
        let proposed = &self.proposal.current.proposed;
        let several = proposed.questions.len() > 1;
        let picks = proposed.questions.iter().enumerate().map(|(at, question)| {
            let chosen = self.answering.chosen.get(at).copied().flatten();
            let said = match chosen.and_then(|place| question.named(place)) {
                Some((letter, option)) => format!("{letter} · {}", option.label.trim()),
                None => "Not chosen".to_string(),
            };
            let said = if several { format!("{}: {said}", question.header.trim()) } else { said };
            div().flex_none().text_color(if chosen.is_some() { colors.text } else { colors.text_faint }).child(said)
        });
        let failed = match &self.answering.sending {
            Sending::Failed(why) => Some(why.clone()),
            Sending::Idle | Sending::Going => None,
        };
        let failed = failed.map(|why| {
            div().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().text_color(colors.error).child(why)
        });
        h_flex()
            .w_full()
            .gap(look::Space::Small)
            .children(picks)
            .child(div().flex_1())
            .children(failed)
            .child(another)
            .child(send)
            .into_any_element()
    }

    /// What was chosen, and the note, muted.
    fn chose_bar(&self, choice: &Choice, cx: &mut Context<Self>) -> AnyElement {
        let colors = look::colors(cx);
        let chosen = self.proposal.current.chosen_in_words().unwrap_or_default();
        let note = Some(choice.note.clone()).filter(|note| !note.is_empty());
        v_flex()
            .w_full()
            .gap(look::Space::XSmall)
            .child(div().child(format!("Chose {chosen}")))
            .children(note.map(|note| div().text_color(colors.text_muted).child(note)))
            .into_any_element()
    }

    /// The banner over the page while nothing can be sent. While a newer revision is held back, that the agent wrote
    /// it, with a button that shows it. While another round is asked for, that the next revision shows here once the
    /// agent writes it, which says nothing of whether it's started, since the request may still wait in the queue. Once
    /// the agent's withdrawn the proposal, that it has, and why.
    pub(super) fn banner(&self, cx: &mut Context<Self>) -> Option<look::Banner> {
        if let Some(newest) = self.held_back() {
            let show = format!("Show revision {newest}");
            let show =
                look::button("show-newest-revision", look::ButtonKind::Borderless, look::ControlSize::Small, show)
                    .off(self.answering.going())
                    .on_click(cx.listener(|this, _, _, cx| this.show_newest(cx)));
            let said = format!("The agent wrote revision {newest}, so this one can't be answered.");
            return Some(look::banner(look::BannerKind::Note).id("proposal-revised-since").child(said).answer(show));
        }
        let (id, said) = match self.proposal.standing() {
            RevisionStanding::AnotherRound(_) => {
                let next = self.proposal.current.number.next();
                (
                    "proposal-another-round",
                    format!("You asked for another round. Revision {next} shows here once the agent writes it."),
                )
            }
            RevisionStanding::Withdrawn(withdrawal) => {
                ("proposal-withdrawn", format!("Withdrawn by the agent: {}", withdrawal.why.trim()))
            }
            RevisionStanding::Waiting | RevisionStanding::Chose(_) | RevisionStanding::Responded => return None,
        };
        Some(look::banner(look::BannerKind::Note).id(id).child(said))
    }

    fn send_button(off: bool, cx: &mut Context<Self>) -> AnyElement {
        look::button("send-proposal-answer", look::ButtonKind::Main, look::ControlSize::Regular, "Send ↵")
            .off(off)
            .on_click(cx.listener(|this, _, _, cx| this.send(cx)))
            .into_any_element()
    }
}

/// That another round was asked for, and the note that says what to change, muted.
fn round_bar(round: &Round, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let asked = h_flex()
        .gap(look::Space::Small)
        .items_center()
        .child(gpui_kit::component::Icon::new(gpui_kit::assets::IconName::RotateCw).sized(look::IconSize::Regular))
        .child("Another round");
    v_flex()
        .w_full()
        .gap(look::Space::XSmall)
        .child(asked)
        .child(div().text_color(colors.text_muted).child(round.note.clone()))
        .into_any_element()
}
