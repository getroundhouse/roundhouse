//! What's been chosen on each question, and the answers that makes. Nothing here draws, so the rules are tested alone.

use std::collections::BTreeMap;

use super::Question;

/// What's been chosen on one question: its options, by position, and whether what's typed in its Other row counts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Choice {
    /// In the options' order, so the answer reads in the order the agent offered them.
    picked: Vec<usize>,
    other: bool,
}

impl Choice {
    /// With one answer allowed, choosing an option replaces whatever was chosen, a typed answer included. With
    /// several, choosing one again takes it back.
    pub(crate) fn pick(&mut self, option: usize, multiple: bool) {
        if !multiple {
            self.picked = vec![option];
            self.other = false;
        } else if let Some(at) = self.picked.iter().position(|&picked| picked == option) {
            self.picked.remove(at);
        } else {
            let at = self.picked.partition_point(|&picked| picked < option);
            self.picked.insert(at, option);
        }
    }

    /// Follows the Other row. Text there is chosen, and with one answer allowed it replaces a chosen option. Clearing it
    /// takes it back.
    pub(crate) fn typed(&mut self, has_text: bool, multiple: bool) {
        if has_text && !multiple {
            self.picked.clear();
        }
        self.other = has_text;
    }

    /// Where several answers are allowed, takes back what's typed, or chooses it again if there's any.
    pub(crate) fn toggle_typed(&mut self, has_text: bool) {
        self.other = !self.other && has_text;
    }

    pub(crate) fn has(&self, option: usize) -> bool {
        self.picked.contains(&option)
    }

    pub(crate) fn other(&self) -> bool {
        self.other
    }

    /// The answer, if anything's chosen: the options' labels, then what's typed, which has to be more than space.
    pub(crate) fn answer(&self, question: &Question, typed: &str) -> Option<String> {
        let labels = self.picked.iter().filter_map(|&option| question.options.get(option)).map(|o| o.label.as_str());
        let typed = Some(typed.trim()).filter(|typed| self.other && !typed.is_empty());
        let parts: Vec<&str> = labels.chain(typed).collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}

/// The answers to send, by the question's text, as the CLI wants them. A question with nothing chosen is left out.
pub(crate) fn answers<'a>(
    answered: impl IntoIterator<Item = (&'a Question, Option<String>)>,
) -> BTreeMap<String, String> {
    answered.into_iter().filter_map(|(question, answer)| Some((question.text.clone(), answer?))).collect()
}

/// The first question after `from` with nothing chosen, going round to the start, or `from` itself if it's the only
/// one. `None` once everything has an answer.
pub(crate) fn next_unanswered(answered: &[Option<String>], from: usize) -> Option<usize> {
    let count = answered.len();
    (1..=count).map(|step| (from + step) % count).find(|&at| answered[at].is_none())
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};
    use serde_json::json;

    use super::{Choice, answers, next_unanswered};
    use crate::questions::{Question, parse};

    fn colors() -> crate::questions::Question {
        let input = json!({"questions":[{"question":"Colors?","options":[{"label":"Red"},{"label":"Green"},{"label":"Blue"}]}]});
        parse(&input).remove(0)
    }

    #[test]
    fn one_answer_replaces_and_several_toggle_in_the_options_order() {
        let question = colors();
        let mut one = Choice::default();
        one.pick(0, false);
        one.pick(2, false);
        assert_eq!(one.answer(&question, ""), Some("Blue".to_string()));

        let mut several = Choice::default();
        several.pick(2, true);
        several.pick(0, true);
        several.pick(1, true);
        several.pick(1, true);
        assert_eq!(several.answer(&question, ""), Some("Red, Blue".to_string()));
        several.pick(0, true);
        several.pick(2, true);
        assert_eq!(several.answer(&question, ""), None, "chose some, then took them all back");
    }

    #[test]
    fn typing_replaces_one_answer_and_joins_several() {
        let question = colors();
        let mut one = Choice::default();
        one.pick(1, false);
        one.typed(true, false);
        assert!(!one.has(1));
        assert_eq!(one.answer(&question, "  Teal "), Some("Teal".to_string()));
        one.pick(0, false);
        assert!(!one.other(), "an option chosen after typing replaces what's typed");
        assert_eq!(one.answer(&question, "Teal"), Some("Red".to_string()));

        let mut several = Choice::default();
        several.pick(1, true);
        several.typed(true, true);
        assert_eq!(several.answer(&question, "Teal"), Some("Green, Teal".to_string()));
        several.typed(false, true);
        assert_eq!(several.answer(&question, ""), Some("Green".to_string()));
    }

    #[test]
    fn only_space_typed_isnt_an_answer() {
        let mut choice = Choice::default();
        choice.typed(true, false);
        assert_eq!(choice.answer(&colors(), "   "), None);
    }

    #[test]
    fn a_typed_answer_can_be_taken_back_and_chosen_again() {
        let mut several = Choice::default();
        several.typed(true, true);
        several.toggle_typed(true);
        assert!(!several.other());
        several.toggle_typed(true);
        assert!(several.other());
        several.toggle_typed(false);
        several.toggle_typed(false);
        assert!(!several.other(), "nothing typed, so nothing to choose");
    }

    fn some_answered() -> impl Strategy<Value = (Vec<Option<String>>, usize)> {
        vec(proptest::option::of("[a-z]{1,3}"), 0..8).prop_flat_map(|answered| {
            let from = 0..answered.len().max(1);
            (Just(answered), from)
        })
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        /// The next question is the first after `from`, going round, that has no answer, and there's none only once
        /// they all have one.
        #[test]
        fn the_next_is_the_first_without_an_answer_going_round((answered, from) in some_answered()) {
            match next_unanswered(&answered, from) {
                None => prop_assert!(answered.iter().all(Option::is_some)),
                Some(next) => {
                    prop_assert!(answered[next].is_none());
                    let between = (1..answered.len()).map(|step| (from + step) % answered.len()).take_while(|&at| at != next);
                    for at in between {
                        prop_assert!(answered[at].is_some(), "{at} comes first and has no answer");
                    }
                }
            }
        }

        /// What's sent is every answer there is, under its question's text, and nothing for a question without one.
        #[test]
        fn every_answer_is_sent_and_nothing_else((answered, _) in some_answered()) {
            let asked: Vec<Question> = (0..answered.len())
                .map(|q| Question { text: format!("Q{q}"), header: None, options: Vec::new(), multiple: false })
                .collect();

            let sent = answers(asked.iter().zip(answered.clone()));

            prop_assert_eq!(sent.len(), answered.iter().flatten().count());
            for (question, answer) in asked.iter().zip(&answered) {
                prop_assert_eq!(sent.get(&question.text), answer.as_ref());
            }
        }
    }
}
