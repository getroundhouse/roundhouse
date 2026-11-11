//! A prompt as one line, for finding it in a long chat.

use serde::{Deserialize, Serialize};

use super::one_line::one_line;
use super::{Place, Seq};
use crate::event::{EventBody, SessionEvent};

/// A recorded prompt as one line: where it is in the log, and how it starts. Every way to make one tidies its text,
/// including reading one from a hub, so a line from a hub that tidies differently still reads as one line. A prompt is
/// one of the agent's own events, so it has a place among them too.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(from = "Sent")]
pub struct PromptLine {
    seq: Seq,
    place: Place,
    text: String,
    files: u32,
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
}

impl From<Sent> for PromptLine {
    fn from(sent: Sent) -> Self {
        Self::new(sent.seq, &sent.text, sent.files).placed(sent.place)
    }
}

impl PromptLine {
    /// The most of a prompt's text a line keeps. Far more than a line in a list shows, and short enough that a chat
    /// with hundreds of prompts, some of them pasted logs, lists them in a small reply.
    pub const MOST_CHARS: usize = 200;

    /// The line for a prompt recorded at `seq` that says `text` and carries `files` files, with no place until
    /// [`PromptLine::placed`] gives it one.
    pub fn new(seq: Seq, text: &str, files: u32) -> Self {
        Self { seq, place: Place::default(), text: one_line(text, Self::MOST_CHARS), files }
    }

    /// The same line, at `place` among the agent's own events.
    #[must_use]
    pub fn placed(self, place: Place) -> Self {
        Self { place, ..self }
    }

    /// The line for `event`, if it's a prompt, as the hub lists it.
    pub fn of(event: &SessionEvent) -> Option<Self> {
        let EventBody::UserPrompt { text, attachments, .. } = &event.body else { return None };
        let files = u32::try_from(attachments.len()).unwrap_or(u32::MAX);
        Some(Self::new(event.seq, text, files).placed(event.place))
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let body = EventBody::UserPrompt { text: "Fix\nit".to_string(), client_msg_id: None, attachments: Vec::new() };

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
    fn a_line_from_a_hub_that_doesnt_record_places_has_none() {
        let sent = serde_json::json!({ "seq": 7, "text": "Fix it", "files": 2 });

        assert_eq!(serde_json::from_value::<PromptLine>(sent).unwrap().place(), Place::default());
    }
}
