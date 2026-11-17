//! One recording, from its start through the final words delivered after it stops. Dropping it cancels both the
//! recording and the task that hears it, so a field can never receive a previous recording's late updates.

use dictation::{DictationError, Ending, Permission, Recording, Update};
use gpui_kit::{Context, Task, Window};

use crate::fine_line::FineLine;
use crate::text::Before;
use crate::{Event, Field, source};

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
            match dictation::microphone_permission() {
                Permission::Allowed => {}
                Permission::NotAsked => return dictation::ask_for_microphone(|_| {}),
                Permission::Denied => return cx.emit(Event::Error(ALLOW_THE_MICROPHONE.to_string())),
            }
        }
        let hints = self.current_hints(cx);
        if let Some((recording, updates)) = source::record(hints, cx) {
            self.start_with(recording, updates, started, window, cx);
        }
    }

    pub(crate) fn start_with(
        &mut self,
        recording: Recording,
        updates: flume::Receiver<Update>,
        started: Started,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.underway.is_some() || !self.can_dictate(cx) {
            return;
        }
        let before = Before::of(self.input.read(cx));
        // Only now, rather than as held Space starts to listen, so a press let go of before it dictates doesn't cut
        // what's playing.
        if source::mutes_output(cx) {
            recording.mute_output();
        }
        let hearing = cx.spawn_in(window, async move |this, cx| {
            while let Ok(update) = updates.recv_async().await {
                if this.update_in(cx, |field, window, cx| field.heard(update, window, cx)).is_err() {
                    return;
                }
            }
            let _ = this.update(cx, |field, cx| {
                let submit = field.underway.take().is_some_and(|u| u.submit);
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
            _hearing: hearing,
        });
        cx.notify();
    }

    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(underway) = self.underway.as_mut().filter(|u| !u.stopping) {
            underway.stopping = true;
            self.line.update(cx, FineLine::settle);
            underway.recording.stop();
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

    pub(crate) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.space_down = None;
        let Some(underway) = self.underway.take() else { return };
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
            Update::Ended(Ending::Heard) => {}
            Update::Ended(Ending::NothingHeard) => cx.emit(Event::Error(NOTHING_HEARD.to_string())),
            Update::Failed(error) => {
                underway.submit = false;
                cx.emit(Event::Error(failure(&error)));
            }
        }
        cx.notify();
    }
}

fn failure(error: &DictationError) -> String {
    match error {
        DictationError::MicrophoneDenied => ALLOW_THE_MICROPHONE.to_string(),
        DictationError::MicrophoneNotAsked => {
            "Dictate again, and allow Roundhouse to use the microphone when asked.".to_string()
        }
        DictationError::NoMicrophone => "There's no microphone to dictate with.".to_string(),
        DictationError::Microphone(_) | DictationError::Engine(_) | DictationError::AudioBacklog => {
            format!("Dictation stopped: {error}.")
        }
    }
}
