//! Answering a proposal in its tab: Choose on each option's card picks it for its question, and the bar at the tab's
//! foot, like the composer, takes a note and sends the answer, once one option is chosen for each question. The app
//! sends it, as [`ProposalEvent::Answer`] asks, and the hub keeps it on the proposal, so once its record says it's
//! answered, the bar says what was chosen instead. What's chosen and written before it's sent is this view's alone.
//! The note can be dictated into, as `dictating` says.

use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, Subscription, TestSupportExt as _, WeakEntity, Window, div, px,
};
use look::{Shaped as _, Typeset as _};
use protocol::client::proposals::ProposalAnswer;
use protocol::model::{Choice, ClientRequestId, OptionLetter, OptionPlace, RevisionStanding};

use super::view::{COLUMN, ProposalView};

/// What the app does for the view.
#[derive(Clone, Debug)]
pub enum ProposalEvent {
    /// Send this answer to the hub, and say how it went with [`ProposalView::sent`].
    Answer(ProposalAnswer),
    /// Say what went wrong, such as dictating into the note.
    Error(String),
}

/// What's chosen and written, and whether it's being sent.
#[derive(Default)]
pub(super) struct Answering {
    /// For each question, the option chosen, if one is.
    pub(super) chosen: Vec<Option<OptionPlace>>,
    pub(super) note: Option<Note>,
    /// The request that sent the answer, with the answer it sent, kept while it's sent and after it failed, so the
    /// same answer sent again, which the hub may have kept after all, is kept once. A changed answer is a new request,
    /// so the hub doesn't take it for the one it kept and leave the change unsaid.
    request: Option<(ClientRequestId, ProposalAnswer)>,
    sending: Sending,
}

/// The note, made the first time the bar is drawn, since an input needs the window, and what dictates into it.
pub(super) struct Note {
    pub(super) input: Entity<TextareaState>,
    pub(super) dictation: Entity<dictation_ui::Field>,
    _events: Vec<Subscription>,
}

/// Where sending the answer is.
#[derive(Default)]
enum Sending {
    /// Not sent, or sent and taken.
    #[default]
    Idle,
    Going,
    /// It couldn't be sent, and why.
    Failed(String),
}

/// What a card's Choose needs: the view, to choose with, what's chosen, and where the revision stands.
pub(super) struct Choosing<'a> {
    pub(super) this: WeakEntity<ProposalView>,
    pub(super) chosen: &'a [Option<OptionPlace>],
    pub(super) standing: RevisionStanding<'a>,
}

impl Choosing<'_> {
    /// The button at the end of option `place`'s card, for question `at`: Choose, or Chosen once it's chosen, while
    /// it waits for the person. Once they've chosen, only Chosen, on the one chosen, and once they've responded
    /// otherwise, none.
    pub(super) fn button(&self, at: usize, place: OptionPlace, letter: OptionLetter) -> Option<AnyElement> {
        let id = ElementId::Name(format!("choose-{at}-{}", place.index()).into());
        match self.standing {
            RevisionStanding::Waiting => {}
            RevisionStanding::Chose(choice) => {
                let chosen = choice.chosen.get(at) == Some(&place);
                return chosen.then(|| chosen_button(id).into_any_element());
            }
            RevisionStanding::Responded => return None,
        }
        if self.chosen.get(at).copied().flatten() == Some(place) {
            let this = self.this.clone();
            let button = chosen_button(id).on_click(move |_, _, cx| {
                let _ = this.update(cx, |view, cx| view.choose(at, None, cx));
            });
            return Some(button.into_any_element());
        }
        let this = self.this.clone();
        let words = format!("Choose {letter}");
        let button =
            look::button(id, look::ButtonKind::Edged, look::ControlSize::Small, words).on_click(move |_, _, cx| {
                let _ = this.update(cx, |view, cx| view.choose(at, Some(place), cx));
            });
        Some(button.into_any_element())
    }
}

/// Chosen, as a choice among a plan's or a dialog's answers, on, with a tick.
fn chosen_button(id: ElementId) -> look::Button {
    look::button(id, look::ButtonKind::Edged, look::ControlSize::Small, "Chosen")
        .icon(gpui_kit::assets::IconName::Check)
        .on(true)
}

impl ProposalView {
    /// Chooses `place` for question `at`, or nothing, in place of what was chosen for it.
    pub(super) fn choose(&mut self, at: usize, place: Option<OptionPlace>, cx: &mut Context<Self>) {
        let questions = self.proposal.current.proposed.questions.len();
        let chosen = &mut self.answering.chosen;
        chosen.resize(questions, None);
        if let Some(slot) = chosen.get_mut(at) {
            *slot = place;
        }
        cx.notify();
    }

    /// Whether something's chosen or written here that hasn't been sent, which showing another proposal would lose.
    pub fn has_unsent(&self, cx: &App) -> bool {
        let noted = self.answering.note.as_ref().is_some_and(|note| {
            !note.input.read(cx).text().to_string().trim().is_empty() || note.dictation.read(cx).has_live_words()
        });
        self.waits() && (noted || self.answering.chosen.iter().any(Option::is_some))
    }

    /// Whether the proposal waits for the person's response.
    pub(super) fn waits(&self) -> bool {
        self.proposal.current.standing() == RevisionStanding::Waiting
    }

    /// Whether it waits for an answer and none is being sent, so one can be given.
    pub(super) fn open(&self) -> bool {
        self.waits() && !matches!(self.answering.sending, Sending::Going)
    }

    /// One option for each question, when one is chosen for each, while it's open.
    pub(super) fn ready(&self) -> Option<Vec<OptionPlace>> {
        let chosen = &self.answering.chosen;
        let questions = self.proposal.current.proposed.questions.len();
        (self.open() && chosen.len() == questions).then(|| chosen.iter().copied().collect::<Option<_>>())?
    }

    /// Asks the app to send the answer, once one option is chosen for each question and until it's answered. The same
    /// answer sent again after it couldn't be is the same request, so if the hub kept it after all, it keeps it once.
    /// While the note's being dictated into, it sends once the last words are in.
    pub(super) fn send(&mut self, cx: &mut Context<Self>) {
        if self.once_dictated(cx) {
            return;
        }
        let Some(chosen) = self.ready() else { return };
        let note = self.answering.note.as_ref().map(|note| note.input.read(cx).text().to_string());
        let mut answer = ProposalAnswer {
            id: self.proposal.id.clone(),
            revision: self.proposal.current.number,
            chosen,
            note: note.unwrap_or_default().trim().to_string(),
            client_request_id: client_core::new_request_id(),
        };
        let same = |sent: &ProposalAnswer| {
            (sent.revision, &sent.chosen, &sent.note) == (answer.revision, &answer.chosen, &answer.note)
        };
        match &self.answering.request {
            Some((request, sent)) if same(sent) => answer.client_request_id = request.clone(),
            _ => self.answering.request = Some((answer.client_request_id.clone(), answer.clone())),
        }
        self.answering.sending = Sending::Going;
        self.follow_dictation(cx);
        cx.emit(ProposalEvent::Answer(answer));
        cx.notify();
    }

    /// How sending the answer went: sent, or why it couldn't be, which the bar says beside Send.
    pub fn sent(&mut self, outcome: Result<(), String>, cx: &mut Context<Self>) {
        self.answering.sending = match outcome {
            Ok(()) => Sending::Idle,
            Err(why) => Sending::Failed(why),
        };
        self.follow_dictation(cx);
        cx.notify();
    }

    fn note(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TextareaState>, Entity<dictation_ui::Field>) {
        if let Some(note) = &self.answering.note {
            return (note.input.clone(), note.dictation.clone());
        }
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 6)
                .submit_on_enter(true)
                .placeholder("Add a note to your answer")
        });
        let typed = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.send(cx);
            }
        });
        let (dictation, [events, changed]) = self.attach_dictation(&input, window, cx);
        let events = vec![typed, events, changed];
        self.answering.note = Some(Note { input: input.clone(), dictation: dictation.clone(), _events: events });
        (input, dictation)
    }

    /// The bar at the tab's foot: the note and what's chosen, with Send, or once it's answered, what was chosen. While
    /// the note's dictated into, its live words show under what's typed, the fine line runs along the bar's bottom, as
    /// along the composer's, and the recording's controls take the place of what's chosen and Send.
    pub(super) fn foot(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (inside, dictation) = match self.proposal.current.standing() {
            RevisionStanding::Waiting => {
                let (input, dictation) = self.note(window, cx);
                let live = dictation.read(cx).live_line(cx).map(|line| {
                    div()
                        .w_full()
                        .pt(look::Space::XSmall)
                        .child(line.px(gpui_kit::component::Size::default().input_px()))
                });
                let field = div().w_full().child(Textarea::new(&input).appearance(false)).children(live);
                let row = match dictation_ui::Field::controls(&dictation, look::ControlSize::Regular, cx) {
                    Some(controls) => controls.into_any_element(),
                    None => self.chosen_row(cx),
                };
                let inside = v_flex().w_full().gap(look::Space::Small).child(field).child(row).into_any_element();
                (inside, Some(dictation))
            }
            RevisionStanding::Chose(choice) => (self.chose_bar(choice, cx), None),
            RevisionStanding::Responded => {
                let inside =
                    div().child("Responded, in a way this version of Roundhouse can't show").into_any_element();
                (inside, None)
            }
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
    }

    /// What's chosen for each question, by its letter and label, and Send.
    fn chosen_row(&self, cx: &mut Context<Self>) -> AnyElement {
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

    fn send_button(off: bool, cx: &mut Context<Self>) -> AnyElement {
        look::button("send-proposal-answer", look::ButtonKind::Main, look::ControlSize::Regular, "Send ↵")
            .off(off)
            .on_click(cx.listener(|this, _, _, cx| this.send(cx)))
            .into_any_element()
    }
}
