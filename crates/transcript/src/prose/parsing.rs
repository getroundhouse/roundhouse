//! When and where a text is parsed: where it's drawn, when what's to be parsed of it is short, and otherwise on another
//! thread, as [`Prose::parsed`] says.

use std::rc::Rc;

use gpui_kit::App;

use super::parse::{self, Again};
use super::{AT_ONCE, ImagesAre, Of, Parsed, Prose, ProseId, Text};

/// How a text is to be parsed on another thread: `source` whole, with its images `images` to draw, or again from where
/// `again` says, as it goes on from `was`, which is what it was parsed as before, if it was, and what it's drawn as
/// until it's in.
pub(super) struct Apart {
    source: String,
    images: ImagesAre,
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
    /// next opened. So is one that says more there than it was parsed as, and one whose images are `images` to draw
    /// where they weren't, as a reply's are in the row of its record, and not while it's written.
    pub(super) fn parsed(
        &self,
        id: &ProseId,
        source: &str,
        of: Of,
        images: ImagesAre,
    ) -> (Option<Rc<Parsed>>, Option<Apart>) {
        let can_go_on = of.can_go_on();
        let mut texts = self.0.borrow_mut();
        let was = match texts.texts.get(id) {
            Some(Text::Parsed { parsed })
                if parsed.source == source && parsed.images == images && (can_go_on || parsed.whole) =>
            {
                return (Some(parsed.clone()), None);
            }
            Some(Text::Parsing { source: parsing, was, .. }) if source.starts_with(parsing.as_str()) => {
                return (was.clone(), None);
            }
            Some(text) => text.was(),
            None => None,
        };
        let goes_on = |was: &&Parsed| can_go_on && was.images == images && source.starts_with(was.source.as_str());
        let again = was.as_deref().filter(goes_on).and_then(|was| parse::again(was, source));
        if again.map_or(source.len(), |again| source.len() - again.byte) > AT_ONCE {
            return (was.clone(), Some(Apart { source: source.to_owned(), images, was, again }));
        }
        let parsed = Rc::new(match (was, again) {
            (Some(was), Some(again)) => parse::gone_on(&was, again, source.to_owned()),
            _ => parse::parse(source.to_owned(), images),
        });
        texts.texts.insert(id.clone(), Text::Parsed { parsed: parsed.clone() });
        texts.parsed_one();
        (Some(parsed), None)
    }

    /// Parses the text known as `id` on another thread, as `apart` says, unless what it says goes on from what's being
    /// parsed of it, and measures the row it's in then again once it's in. It says what it said, so there's nothing
    /// new to find in it.
    pub(super) fn parse_apart(&self, id: ProseId, apart: Apart, cx: &mut App) {
        let Apart { source, images, was, again } = apart;
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
                    let (given, again) = (given.clone(), *again);
                    background.spawn(async move { parse::rest(&given, again, images) }).await
                }
                None => None,
            };
            let parsed = match (from, rest) {
                (Some((was, again)), Some(rest)) => parse::joined(&was, again, given, rest),
                _ => background.spawn(async move { parse::parse(given, images) }).await,
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
