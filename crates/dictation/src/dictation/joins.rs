//! How the live text joins one piece of a recording to the next, and how a sentence's final text ends. The live model
//! hears what follows a word before it gives it, so it punctuates a join, often only once it has heard the next word
//! begin, where a sentence heard whole on its own can only end as a sentence ends.

use crate::Heard;

/// The marks that end a sentence, and those that join two parts of one.
const ENDS_A_SENTENCE: [char; 3] = ['.', '?', '!'];
const JOINS: [char; 3] = [',', ';', ':'];

/// Whether `heard` is a mark that ends a sentence or joins two parts of one, rather than a word, part of one, or a
/// symbol such as `%` or `$`, which belongs to the word it's beside.
pub(super) fn is_mark(heard: &Heard) -> bool {
    heard.text.trim().chars().all(|c| ENDS_A_SENTENCE.contains(&c) || JOINS.contains(&c))
}

/// How the live text joins two pieces.
pub(super) struct Join {
    /// The marks between them, such as `,`, or `.` where a sentence ends.
    pub(super) marks: String,
    /// How the word after the marks begins, once the live text has given it.
    pub(super) next: Option<Begins>,
}

/// How a word begins, as the live text has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Begins {
    /// In lower case, so a sentence cut short before it carries on in lower case.
    LowerCase,
    /// With a capital, as a name or `I` does, or with a digit or a symbol, which is kept.
    Otherwise,
}

impl Join {
    /// How the live text `before` a join is joined to what's `after` it: the marks at the end of what's before, and
    /// those after it before the next word.
    pub(super) fn of<'h>(before: &[Heard], after: impl IntoIterator<Item = &'h Heard>) -> Self {
        let at_its_end = before.iter().rev().take_while(|heard| is_mark(heard)).count();
        let mut marks: String = before[before.len() - at_its_end..].iter().map(|heard| heard.text.trim()).collect();
        for heard in after {
            if !is_mark(heard) {
                let lower = heard.text.trim_start().starts_with(char::is_lowercase);
                return Self { marks, next: Some(if lower { Begins::LowerCase } else { Begins::Otherwise }) };
            }
            marks.push_str(heard.text.trim());
        }
        Self { marks, next: None }
    }

    /// The mark it ends a sentence with, if it does.
    pub(super) fn ends_sentence(&self) -> Option<char> {
        self.marks.chars().find(|c| ENDS_A_SENTENCE.contains(c))
    }
}

/// How a sentence that's final ends.
#[derive(Debug)]
pub(super) enum Ends {
    /// With `mark`, such as `.` or `?`: where the live text ended it, or where the recording ended and the live text
    /// ends a sentence there.
    Marked(char),
    /// Cut short, since it would be too long to be heard whole with the next piece, and joined to the rest as the live
    /// text joins them: with the marks between them, such as `,`, and the next word beginning as it does.
    Cut { marks: String, next: Begins },
    /// Where the recording does, with no mark from the live text.
    Unmarked,
}

/// What of a sentence goes in: its final `text`, heard whole, beginning in lower case when it `carries_on` a
/// sentence cut short, and ending as `ends` says. With it, whether the next carries this one on. `None` when nothing
/// but marks was heard in it.
pub(super) fn final_text(text: &str, carries_on: bool, ends: Ends) -> Option<(String, bool)> {
    let text = text.trim();
    let mut text = if carries_on { lower_case_first(text) } else { text.to_string() };
    match ends {
        // Its own mark, if the engine gave one, or else the live text's.
        Ends::Marked(mark) => {
            text.truncate(text.trim_end_matches(JOINS).len());
            if !text.ends_with(ENDS_A_SENTENCE) {
                text.push(mark);
            }
            words(text, false)
        }
        // Heard whole on its own, it ends as a sentence would, so it ends as the live text has it go on instead.
        Ends::Cut { marks, next } => {
            text.truncate(text.trim_end_matches(|c| ENDS_A_SENTENCE.contains(&c) || JOINS.contains(&c)).len());
            let carried = words(text, next == Begins::LowerCase);
            carried.map(|(text, carries_on)| (text + &marks, carries_on))
        }
        Ends::Unmarked => words(text, false),
    }
}

/// `text`, unless it's no more than marks.
fn words(text: String, carries_on: bool) -> Option<(String, bool)> {
    text.chars()
        .any(|c| !(c.is_whitespace() || ENDS_A_SENTENCE.contains(&c) || JOINS.contains(&c)))
        .then_some((text, carries_on))
}

/// `text` with its first letter in lower case, unless its first word has another capital, as `API` and `GitHub` do.
fn lower_case_first(text: &str) -> String {
    let first_word = text.split_whitespace().next().unwrap_or_default();
    if first_word.chars().skip(1).any(char::is_uppercase) {
        return text.to_string();
    }
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_lowercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::{Begins, Ends, Join, final_text, is_mark};
    use crate::{Heard, Samples};

    fn heard(texts: &[&str]) -> Vec<Heard> {
        texts.iter().map(|text| Heard { text: (*text).to_string(), at: Samples(0) }).collect()
    }

    fn cut(marks: &str, next: Begins) -> Ends {
        Ends::Cut { marks: marks.to_string(), next }
    }

    #[test]
    fn marks_are_the_punctuation_that_ends_or_joins_and_not_symbols() {
        for mark in [".", "?", "!", ",", ";", ":", " .", "..."] {
            assert!(is_mark(&heard(&[mark])[0]), "{mark:?}");
        }
        for text in [" a", "ing", "%", " $", "'", "¿", " 5"] {
            assert!(!is_mark(&heard(&[text])[0]), "{text:?}");
        }
    }

    #[test]
    fn a_join_has_the_marks_either_side_of_it_and_how_the_next_word_begins() {
        let join = Join::of(&heard(&[" did", " it", " pass", "?"]), &heard(&[" I", " think"]));
        assert_eq!((join.marks.as_str(), join.next, join.ends_sentence()), ("?", Some(Begins::Otherwise), Some('?')));

        let join = Join::of(&heard(&[" we", " should"]), &heard(&[",", " so"]));
        assert_eq!((join.marks.as_str(), join.next, join.ends_sentence()), (",", Some(Begins::LowerCase), None));

        let join = Join::of(&heard(&[" fifty", "%"]), &heard(&[" and"]));
        assert_eq!(join.marks, "", "a symbol is the word's");

        let join = Join::of(&heard(&[" it"]), &heard(&["!"]));
        assert_eq!((join.next, join.ends_sentence()), (None, Some('!')), "nothing after it yet");
    }

    #[test]
    fn a_sentence_keeps_its_own_mark_or_takes_the_live_texts() {
        for mark in ['.', '?', '!'] {
            assert_eq!(final_text("Should we", false, Ends::Marked(mark)), Some((format!("Should we{mark}"), false)));
            assert_eq!(final_text("Should we,", false, Ends::Marked(mark)), Some((format!("Should we{mark}"), false)));
        }
        assert_eq!(final_text("Should we.", false, Ends::Marked('?')), Some(("Should we.".to_string(), false)));
        assert_eq!(final_text("Should we", false, Ends::Unmarked), Some(("Should we".to_string(), false)));
    }

    #[test]
    fn a_sentence_cut_short_ends_as_the_live_text_goes_on() {
        let carried = final_text("We should.", false, cut(",", Begins::LowerCase));
        assert_eq!(carried, Some(("We should,".to_string(), true)));
        let before_a_capital = final_text("We should.", false, cut("", Begins::Otherwise));
        assert_eq!(before_a_capital, Some(("We should".to_string(), false)));
        let before_a_symbol = final_text("Fifty%.", false, cut("", Begins::LowerCase));
        assert_eq!(before_a_symbol, Some(("Fifty%".to_string(), true)));
    }

    #[test]
    fn what_carries_on_begins_in_lower_case_unless_its_first_word_is_a_name() {
        let carried = final_text("Points to the right place.", true, Ends::Unmarked);
        assert_eq!(carried, Some(("points to the right place.".to_string(), false)));
        for name in ["API calls.", "GitHub runs it."] {
            assert_eq!(final_text(name, true, Ends::Unmarked), Some((name.to_string(), false)));
        }
    }

    #[test]
    fn a_sentence_of_nothing_but_marks_sends_nothing() {
        assert_eq!(final_text("", false, Ends::Marked('.')), None);
        assert_eq!(final_text(".", false, cut(",", Begins::LowerCase)), None);
        assert_eq!(final_text(" , ", false, Ends::Marked('.')), None);
    }
}
