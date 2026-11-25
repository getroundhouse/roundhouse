//! One recording, from its start through the final words delivered after it stops. Dropping it cancels both the
//! recording and the task that hears it, so a field can never receive a previous recording's late updates.

use dictation::{DictationError, Ending, Recording, Update};
use gpui_kit::{Context, Task, Window};
use sound::{MicrophoneError, Permission, Quieting};

use crate::fine_line::FineLine;
use crate::logged::{self, Logged};
use crate::source::Recorded;
use crate::text::Before;
use crate::{Event, Field, keyboard, source};

pub const NOTHING_HEARD: &str = "No speech was heard.";
const ALLOW_THE_MICROPHONE: &str =
    "Allow Roundhouse to use the microphone in System Settings › Privacy & Security › Microphone, then try again.";

pub(crate) struct Underway {
    pub(crate) started: Started,
    pub(crate) stopping: bool,
    pub(crate) live: String,
    before: Before,
    pub(crate) submit: bool,
    pub(crate) recording: Recording,
    logged: Logged,
    _hearing: Task<()>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Started {
    Key,
    Button,
}

impl Field {
    pub(crate) fn start(&mut self, started: Started, window: &mut Window, cx: &mut Context<Self>) {
        if self.underway.is_some() || !self.can_dictate(cx) {
            return;
        }
        self.space_down = None;
        source::claim(cx);
        if source::uses_microphone(cx) {
            match sound::microphone_permission() {
                Permission::Allowed => {}
                Permission::NotAsked => return sound::ask_for_microphone(|_| {}),
                Permission::Denied => {
                    logged::microphone_denied(self.place);
                    return cx.emit(Event::Error(ALLOW_THE_MICROPHONE.to_string()));
                }
            }
        }
        let hints = self.current_hints(cx);
        if let Some(recorded) = source::record(hints, cx) {
            self.start_with(recorded, started, window, cx);
        }
    }

    pub(crate) fn start_with(
        &mut self,
        recorded: Recorded,
        started: Started,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.underway.is_some() || !self.can_dictate(cx) {
            return;
        }
        let Recorded { recording, updates, hinted } = recorded;
        let logged = Logged::start(self.place, hinted);
        let before = Before::of(self.input.read(cx));
        // Only now, rather than as held Space starts to listen, so a press let go of before it dictates doesn't cut
        // what's playing.
        for how in Quieting::ALL.into_iter().filter(|how| source::quiets(*how, cx)) {
            recording.quiet(how);
        }
        cx.emit(Event::Started);
        let hearing = cx.spawn_in(window, async move |this, cx| {
            while let Ok(update) = updates.recv_async().await {
                if this.update_in(cx, |field, window, cx| field.heard(update, window, cx)).is_err() {
                    return;
                }
            }
            let _ = this.update(cx, |field, cx| {
                let underway = field.underway.take();
                // Asked only now, so the system's question doesn't stop the first recording held to dictate, as
                // another app coming in front would.
                if underway.as_ref().is_some_and(|u| u.started == Started::Key) {
                    keyboard::ask_to_see_the_keyboard(cx);
                }
                let submit = underway.is_some_and(|u| u.submit);
                field.line.update(cx, FineLine::hide);
                cx.emit(Event::Ended { submit });
                cx.notify();
            });
        });
        self.line.update(cx, |line, cx| line.listen(recording.levels(), cx));
        self.underway = Some(Underway {
            started,
            stopping: false,
            live: String::new(),
            before,
            submit: false,
            recording,
            logged,
            _hearing: hearing,
        });
        cx.notify();
    }

    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(underway) = self.underway.as_mut().filter(|u| !u.stopping) {
            underway.stopping = true;
            self.line.update(cx, FineLine::settle);
            underway.recording.stop();
            underway.logged.stopped();
            cx.notify();
        }
    }

    /// Starts from the button or stops, independently of whether the input currently has keyboard focus.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.underway.is_some() {
            self.stop(cx);
        } else {
            self.start(Started::Button, window, cx);
        }
    }

    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.space_down = None;
        let Some(mut underway) = self.underway.take() else { return };
        underway.logged.cancelled();
        self.line.update(cx, FineLine::hide);
        self.put_back(&underway.before, window, cx);
        cx.emit(Event::Ended { submit: false });
        cx.notify();
    }

    fn heard(&mut self, update: Update, window: &mut Window, cx: &mut Context<Self>) {
        let Some(underway) = self.underway.as_mut() else { return };
        match update {
            Update::Hearing(text) => underway.live = text,
            Update::Final(text) => self.put_in(&text, window, cx),
            Update::Ended(ending) => {
                underway.logged.ended(ending);
                if ending == Ending::NothingHeard {
                    cx.emit(Event::Error(NOTHING_HEARD.to_string()));
                }
            }
            Update::Failed(error) => {
                underway.submit = false;
                underway.logged.failed(&error);
                cx.emit(Event::Error(failure(&error)));
            }
        }
        cx.notify();
    }
}

fn failure(error: &DictationError) -> String {
    match error {
        DictationError::Microphone(MicrophoneError::Denied) => ALLOW_THE_MICROPHONE.to_string(),
        DictationError::Microphone(MicrophoneError::NotAsked) => {
            "Dictate again, and allow Roundhouse to use the microphone when asked.".to_string()
        }
        DictationError::Microphone(MicrophoneError::Missing) => "There's no microphone to dictate with.".to_string(),
        DictationError::Microphone(MicrophoneError::Failed(_))
        | DictationError::Engine(_)
        | DictationError::AudioBacklog => {
            format!("Dictation stopped: {error}.")
        }
    }
}
