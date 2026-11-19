//! Answering a proposal in its tab: Choose on each option's card picks it for its question, and the bar at the tab's
//! foot, like the composer, takes a note and sends the answer, once one option is chosen for each question, or asks
//! for another round without choosing, once the note says what to change. The app sends it, as
//! [`ProposalEvent::Send`] asks, and the hub keeps it on the proposal, so once its record says it's answered, the bar
//! says what was chosen, or that another round was asked for, instead. What's chosen and written before it's sent is
//! this view's alone. The note can be dictated into, as `dictating` says, and `bar` draws the bar.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, IntoElement as _, Subscription, WeakEntity, Window,
};
use protocol::client::proposals::{ProposalAnotherRound, ProposalAnswer};
use protocol::model::{ClientRequestId, OptionLetter, OptionPlace, RevisionStanding};

use super::view::ProposalView;

/// What the app does for the view.
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalEvent {
    /// Send this to the hub, and say how it went with [`ProposalView::sent`].
    Send(ProposalRequest),
    /// Say what went wrong, such as dictating into the note.
    Error(String),
}

/// What the view asks the app to send the hub, and to say how it went with [`ProposalView::sent`].
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalRequest {
    Answer(ProposalAnswer),
    AnotherRound(ProposalAnotherRound),
}

impl ProposalRequest {
    fn id(&mut self) -> &mut ClientRequestId {
        match self {
            ProposalRequest::Answer(answer) => &mut answer.client_request_id,
            ProposalRequest::AnotherRound(round) => &mut round.client_request_id,
        }
    }
}

/// What's chosen and written, and whether it's being sent.
#[derive(Default)]
pub(super) struct Answering {
    /// For each question, the option chosen, if one is.
    pub(super) chosen: Vec<Option<OptionPlace>>,
    pub(super) note: Option<Note>,
    /// The request that sent the answer, or asked for another round, kept while it's sent and after it failed, so the
    /// same sent again, which the hub may have kept after all, is kept once. A changed one is a new request, so the hub
    /// doesn't take it for the one it kept and leave the change unsaid.
    request: Option<ProposalRequest>,
    pub(super) sending: Sending,
}

/// The note, made the first time the bar is drawn, since an input needs the window, and what dictates into it.
pub(super) struct Note {
    pub(super) input: Entity<TextareaState>,
    pub(super) dictation: Entity<dictation_ui::Field>,
    _events: Vec<Subscription>,
}

/// Where sending the answer is.
#[derive(Default)]
pub(super) enum Sending {
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
    /// it waits for the person. Once they've chosen, only Chosen, on the one chosen, and once they've asked for
    /// another round, or responded otherwise, or the agent's withdrawn it, none.
    pub(super) fn button(&self, at: usize, place: OptionPlace, letter: OptionLetter) -> Option<AnyElement> {
        let id = ElementId::Name(format!("choose-{at}-{}", place.index()).into());
        match self.standing {
            RevisionStanding::Waiting => {}
            RevisionStanding::Chose(choice) => {
                let chosen = choice.chosen.get(at) == Some(&place);
                return chosen.then(|| chosen_button(id).into_any_element());
            }
            RevisionStanding::AnotherRound(_) | RevisionStanding::Responded | RevisionStanding::Withdrawn(_) => {
                return None;
            }
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
        self.proposal.standing() == RevisionStanding::Waiting
    }

    /// Whether an answer or a request for another round can be sent: while it's not answered and not being sent.
    pub(super) fn open(&self) -> bool {
        self.waits() && !matches!(self.answering.sending, Sending::Going)
    }

    /// One option for each question, when one is chosen for each, while an answer can be sent.
    pub(super) fn ready(&self) -> Option<Vec<OptionPlace>> {
        let chosen = &self.answering.chosen;
        let questions = self.proposal.current.proposed.questions.len();
        (self.open() && chosen.len() == questions).then(|| chosen.iter().copied().collect::<Option<_>>())?
    }

    /// What's written in the note, trimmed.
    fn noted(&self, cx: &App) -> String {
        let note = self.answering.note.as_ref().map(|note| note.input.read(cx).text().to_string());
        note.unwrap_or_default().trim().to_string()
    }

    /// The note, when it says something, while another round can be asked for.
    pub(super) fn round_ready(&self, cx: &App) -> Option<String> {
        Some(self.noted(cx)).filter(|note| self.open() && !note.is_empty())
    }

    /// Asks the app to send the answer, once one option is chosen for each question and until it's answered. While
    /// the note's being dictated into, it sends once the last words are in.
    pub(super) fn send(&mut self, cx: &mut Context<Self>) {
        if self.once_dictated(cx) {
            return;
        }
        let Some(chosen) = self.ready() else { return };
        let answer = ProposalAnswer {
            id: self.proposal.id.clone(),
            revision: self.proposal.current.number,
            chosen,
            note: self.noted(cx),
            client_request_id: client_core::new_request_id(),
        };
        self.request(ProposalRequest::Answer(answer), cx);
    }

    /// Asks the app to ask for another round, without choosing, once the note says what to change, and until it's
    /// answered.
    pub(super) fn another_round(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.round_ready(cx) else { return };
        let round = ProposalAnotherRound {
            id: self.proposal.id.clone(),
            revision: self.proposal.current.number,
            note,
            client_request_id: client_core::new_request_id(),
        };
        self.request(ProposalRequest::AnotherRound(round), cx);
    }

    /// Asks the app to send `request`. The same sent again after it couldn't be is the same request, so if the hub
    /// kept it after all, it keeps it once.
    fn request(&mut self, mut request: ProposalRequest, cx: &mut Context<Self>) {
        if let Some(sent) = &mut self.answering.request {
            let mut again = request.clone();
            *again.id() = sent.id().clone();
            if again == *sent {
                request = again;
            }
        }
        self.answering.request = Some(request.clone());
        self.answering.sending = Sending::Going;
        self.follow_dictation(cx);
        cx.emit(ProposalEvent::Send(request));
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

    pub(super) fn note(
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
        let typed = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.send(cx),
            // Another round waits for a note.
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } | InputEvent::Focus | InputEvent::Blur => {}
        });
        let (dictation, [events, changed]) = self.attach_dictation(&input, window, cx);
        let events = vec![typed, events, changed];
        self.answering.note = Some(Note { input: input.clone(), dictation: dictation.clone(), _events: events });
        (input, dictation)
    }
}
