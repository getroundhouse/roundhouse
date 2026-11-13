//! When and where a text is parsed: where it's drawn, when what's to be parsed of it is short, and otherwise on another
//! thread, as [`Prose::parsed`] says.

use std::rc::Rc;

use gpui_kit::App;

use super::parse::{self, Again};
use super::{AT_ONCE, Of, Parsed, Prose, ProseId, Text};

/// How a text is to be parsed on another thread: `source` whole, or again from where `again` says, as it goes on from
/// `was`, which is what it was parsed as before, if it was, and what it's drawn as until it's in.
pub(super) struct Apart {
    source: String,
    was: Option<Rc<Parsed>>,
    again: Option<Again>,
}

impl Prose {
    /// What the text known as `id` is drawn as now that it says `source`, if it has a parse to draw, and how it's to be
    /// parsed on another thread, if it is. A text is parsed where it's drawn when what's to be parsed of it is short:
    /// all of it, or for one that goes on from what it was parsed as, what's parsed again from near its end. A longer
    /// one is parsed on another thread, and is drawn as it was meanwhile, if it was parsed before, so it doesn't go
    /// blank between, as a reply would if its record said other than what was written of it. While a parse is on
    /// another thread, another isn't started for a text that goes on from what that's parsing, so a text that goes on
    /// faster than a parse ends, such as one the agent is writing, is drawn as each parse comes in.
    ///
    /// Only a text drawn where it can go on, at the end of the chat, as [`Of::can_go_on`] says, is parsed again from
    /// near its end, since what that makes of it can differ from a whole parse, as [`parse::again`] says. Drawn
    /// anywhere else, one joined from such parses is parsed whole, once, as a reply is the first time it's drawn in
    /// the row of its record, so it's drawn there as it will be whenever it's parsed afresh, such as when its chat is
    /// next opened. So is one that says more there than it was parsed as.
    pub(super) fn parsed(&self, id: &ProseId, source: &str, of: Of) -> (Option<Rc<Parsed>>, Option<Apart>) {
        let can_go_on = of.can_go_on();
        let mut texts = self.0.borrow_mut();
        let was = match texts.texts.get(id) {
            Some(Text::Parsed { parsed }) if parsed.source == source && (can_go_on || parsed.whole) => {
                return (Some(parsed.clone()), None);
            }
            Some(Text::Parsing { source: parsing, was, .. }) if source.starts_with(parsing.as_str()) => {
                return (was.clone(), None);
            }
            Some(text) => text.was(),
            None => None,
        };
        let gone_on = was.as_deref().filter(|was| can_go_on && source.starts_with(was.source.as_str()));
        let again = gone_on.and_then(|was| parse::again(was, source));
        if again.map_or(source.len(), |again| source.len() - again.byte) > AT_ONCE {
            return (was.clone(), Some(Apart { source: source.to_owned(), was, again }));
        }
        let parsed = Rc::new(match (was, again) {
            (Some(was), Some(again)) => parse::gone_on(&was, again, source.to_owned()),
            _ => parse::parse(source.to_owned()),
        });
        texts.texts.insert(id.clone(), Text::Parsed { parsed: parsed.clone() });
        texts.parsed_one();
        (Some(parsed), None)
    }

    /// Parses the text known as `id` on another thread, as `apart` says, unless what it says goes on from what's being
    /// parsed of it, and measures the row it's in then again once it's in. It says what it said, so there's nothing
    /// new to find in it.
    pub(super) fn parse_apart(&self, id: ProseId, apart: Apart, cx: &mut App) {
        let Apart { source, was, again } = apart;
        let mut texts = self.0.borrow_mut();
        if texts.texts.get(&id).is_some_and(|text| text.parsing().is_some_and(|parsing| source.starts_with(parsing))) {
            return;
        }
        let (prose, redrawn, given, known) =
            (Rc::downgrade(&self.0), texts.redrawn.clone(), source.clone(), id.clone());
        let from = was.clone().zip(again);
        let parsing = cx.spawn(async move |cx| {
            let background = cx.background_executor().clone();
            let rest = match &from {
                Some((_, again)) => {
                    let rest = given[again.byte..].to_owned();
                    background.spawn(async move { parse::read(&rest) }).await
                }
                None => None,
            };
            let parsed = match (from, rest) {
                (Some((was, again)), Some(rest)) => parse::joined(&was, again, given, rest),
                _ => background.spawn(async move { parse::parse(given) }).await,
            };
            let Some(prose) = prose.upgrade() else { return };
            // What it replaces holds this task, which goes once it's done.
            let (_was, place) = {
                let mut texts = prose.borrow_mut();
                if texts.texts.get(&known).and_then(Text::parsing) != Some(parsed.source.as_str()) {
                    return;
                }
                let place = texts.places.of(&known);
                (texts.texts.insert(known, Text::Parsed { parsed: Rc::new(parsed) }), place)
            };
            if let Some(at) = place {
                cx.update(|cx| redrawn(at.of, cx));
            }
        });
        texts.texts.insert(id, Text::Parsing { source, was, _task: parsing });
    }
}

#[cfg(test)]
mod tests;
