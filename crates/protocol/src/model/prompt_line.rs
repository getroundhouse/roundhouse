//! A prompt as one line, for finding it in a long chat.

use serde::{Deserialize, Serialize};

use super::{Place, Seq};
use crate::event::{EventBody, SessionEvent};
use crate::short_text::one_line;

/// A recorded prompt as one line: where it is in the log, where it was settled, and how it starts. Every way to make
/// one tidies its text, including reading one from a hub, so a line from a hub that tidies differently still reads as
/// one line. A prompt is one of the agent's own events, so it has a place among them too.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(from = "Sent")]
pub struct PromptLine {
    seq: Seq,
    place: Place,
    text: String,
    files: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    settled: Option<SettledAt>,
}

/// Where the event is that settled a prompt: the last `PromptTaken` or `PromptDropped` that names it, such as where the
/// agent took it after it was dropped with a lost worker. A chat draws the prompt there, after what the agent did
/// before then.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettledAt {
    pub seq: Seq,
    /// Among the agent's own events, or 0 from a hub that doesn't record places.
    #[serde(default)]
    pub place: Place,
}

impl SettledAt {
    /// Where `event` is, if it settles a prompt, and the prompt it settles.
    pub fn of(event: &SessionEvent) -> Option<(Seq, Self)> {
        let (EventBody::PromptTaken { prompt } | EventBody::PromptDropped { prompt }) = event.body else { return None };
        Some((prompt, Self { seq: event.seq, place: event.place }))
    }
}

/// A line as it's sent, before it's tidied.
#[derive(Deserialize)]
struct Sent {
    seq: Seq,
    /// A hub from before places sends none.
    #[serde(default)]
    place: Place,
    text: String,
    files: u32,
    /// A hub from before it said where each prompt was settled sends none.
    #[serde(default)]
    settled: Option<SettledAt>,
}

impl From<Sent> for PromptLine {
    fn from(sent: Sent) -> Self {
        let line = Self::new(sent.seq, &sent.text, sent.files).placed(sent.place);
        Self { settled: sent.settled, ..line }
    }
}

impl PromptLine {
    /// The most of a prompt's text a line keeps. Far more than a line in a list shows, and short enough that a chat
    /// with hundreds of prompts, some of them pasted logs, lists them in a small reply.
    pub const MOST_CHARS: usize = 200;

    /// The line for a prompt recorded at `seq` that says `text` and carries `files` files, with no place until
    /// [`PromptLine::placed`] gives it one.
    pub fn new(seq: Seq, text: &str, files: u32) -> Self {
        Self { seq, place: Place::default(), text: one_line(text, Self::MOST_CHARS), files, settled: None }
    }

    /// The same line, at `place` among the agent's own events.
    #[must_use]
    pub fn placed(self, place: Place) -> Self {
        Self { place, ..self }
    }

    /// The same line, settled `at`.
    #[must_use]
    pub fn settled_at(self, at: SettledAt) -> Self {
        Self { settled: Some(at), ..self }
    }

    /// The same line, saying nothing of where it was settled, for keeping apart from what keeps that.
    #[must_use]
    pub fn unsettled(self) -> Self {
        Self { settled: None, ..self }
    }

    /// The line for `event`, if it's a prompt, as the hub lists it.
    pub fn of(event: &SessionEvent) -> Option<Self> {
        let EventBody::UserPrompt { prompt, .. } = &event.body else { return None };
        let files = u32::try_from(prompt.files.len()).unwrap_or(u32::MAX);
        Some(Self::new(event.seq, &prompt.text, files).placed(event.place))
    }

    /// Where the prompt is in the log.
    pub fn seq(&self) -> Seq {
        self.seq
    }

    /// Where the prompt is among the agent's own events, or 0 from a hub that doesn't record places.
    pub fn place(&self) -> Place {
        self.place
    }

    /// The start of what was typed, as it reads on one line, tidied as a title is: each run of whitespace is one space,
    /// and characters that show as nothing or reorder what follows are gone. It stops at [`PromptLine::MOST_CHARS`].
    /// Empty for a prompt that only carries files.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How many files it carries.
    pub fn files(&self) -> u32 {
        self.files
    }

    /// Where it was settled, as the hub listed it. `None` while the agent hasn't taken it, from a hub that doesn't
    /// say, and for a line made from the prompt's event, which can't.
    pub fn settled(&self) -> Option<SettledAt> {
        self.settled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::Prompt;

    fn line(text: &str) -> String {
        PromptLine::new(4, text, 0).text
    }

    #[test]
    fn whitespace_of_any_kind_becomes_one_space() {
        assert_eq!(line("  Fix this:\n\n\tcargo  test\r\n"), "Fix this: cargo test");
    }

    #[test]
    fn what_shows_as_nothing_or_reorders_the_line_is_left_out() {
        assert_eq!(line("\u{1b}[31merror\u{1b}[0m"), "[31merror[0m", "a terminal's colour codes keep only their text");
        assert_eq!(line("abc\u{202E}def"), "abcdef", "nothing turns the rest of the row backwards");
    }

    #[test]
    fn a_long_prompt_stops_at_the_limit_on_a_whole_character() {
        let long = "é".repeat(PromptLine::MOST_CHARS + 50);

        assert_eq!(line(&long).chars().count(), PromptLine::MOST_CHARS);
    }

    #[test]
    fn a_space_at_the_limit_is_left_off() {
        let text = format!("{} next", "a".repeat(PromptLine::MOST_CHARS - 1));

        assert_eq!(line(&text), "a".repeat(PromptLine::MOST_CHARS - 1));
    }

    #[test]
    fn a_prompt_has_a_line_at_its_place_and_nothing_else_does() {
        let event = |seq, place, body| SessionEvent {
            session_id: "s".into(),
            seq,
            ts_ms: 0,
            run: None,
            subagent: None,
            place: Place::new(place),
            body,
            raw: None,
        };
        let body = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("Fix\nit") };

        assert_eq!(PromptLine::of(&event(4, 3, body)), Some(PromptLine::new(4, "Fix it", 0).placed(Place::new(3))));
        assert_eq!(PromptLine::of(&event(2, 2, EventBody::PromptTaken { prompt: 1 })), None);
    }

    #[test]
    fn a_line_read_from_a_hub_is_tidied_and_cut() {
        let long = "x".repeat(PromptLine::MOST_CHARS + 1);
        let sent = serde_json::json!({ "seq": 7, "text": format!("Fix\u{202E}\n{long}"), "files": 2 });

        let line: PromptLine = serde_json::from_value(sent).unwrap();

        assert_eq!(line, PromptLine::new(7, &format!("Fix {long}"), 2));
        assert!(line.text().starts_with("Fix x"), "{line:?}");
        assert_eq!(line.text().chars().count(), PromptLine::MOST_CHARS);
    }

    #[test]
    fn a_line_goes_to_a_client_with_the_fields_it_reads() {
        let line = PromptLine::new(7, "Fix it", 2);

        assert_eq!(
            serde_json::to_value(line.placed(Place::new(3))).unwrap(),
            serde_json::json!({ "seq": 7, "place": 3, "text": "Fix it", "files": 2 })
        );
    }

    #[test]
    fn a_line_says_where_it_was_settled_and_one_from_a_hub_that_doesnt_say_has_nowhere() {
        let at = SettledAt { seq: 9, place: Place::new(6) };
        let line = PromptLine::new(7, "Fix it", 2).placed(Place::new(3)).settled_at(at);

        let sent = serde_json::to_value(&line).unwrap();

        assert_eq!(sent["settled"], serde_json::json!({ "seq": 9, "place": 6 }));
        assert_eq!(serde_json::from_value::<PromptLine>(sent).unwrap(), line);
        let older = serde_json::json!({ "seq": 7, "text": "Fix it", "files": 2 });
        assert_eq!(serde_json::from_value::<PromptLine>(older).unwrap().settled(), None);
    }

    #[test]
    fn a_line_from_a_hub_that_doesnt_record_places_has_none() {
        let sent = serde_json::json!({ "seq": 7, "text": "Fix it", "files": 2 });

        assert_eq!(serde_json::from_value::<PromptLine>(sent).unwrap().place(), Place::default());
    }
}
