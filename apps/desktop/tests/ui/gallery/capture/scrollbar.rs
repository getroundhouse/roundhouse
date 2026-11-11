//! The chat's scrollbar, which shows for two seconds after the chat scrolls, by the wall clock, which the gallery
//! can't hold still. A picture taken sooner would show it or not by how fast it was taken, so one whose frame takes it
//! in is taken again once it has faded.

use std::time::{Duration, Instant};

use gpui_kit::base::ScrollbarHandle as _;
use gpui_kit::{EntityId, Pixels, px};

use super::{BETWEEN_PICTURES, Driver, Picture, TIMEOUT, selected_pane};
use crate::gallery::frame;
use crate::gallery::steps::Screen;

/// Longer than the kit's scrollbar shows after a scroll, two seconds, and then takes to fade.
const LONGER_THAN_IT_SHOWS: Duration = Duration::from_secs(3);

/// Where the chat showing was scrolled to when the last frame was drawn, and when that last changed, as the kit's
/// scrollbar sees it.
#[derive(Default)]
pub(super) struct ChatScroll {
    chat: Option<EntityId>,
    offset: Pixels,
    changed: Option<Instant>,
}

impl Driver {
    /// `picture`, or if the chat scrolled lately and the screen's frame takes in the strip where its scrollbar is
    /// drawn, the screen's picture once the scrollbar has faded.
    pub(super) fn without_the_chats_scrollbar(&mut self, screen: &Screen, picture: Picture) -> Picture {
        if !self.chat_scrolled_lately() {
            return picture;
        }
        let shows = self.cx.update_window(self.handle, |_, window, _| {
            frame::shows_the_chats_scrollbar(&screen.frame, window, &picture.image)
        });
        if !shows.unwrap() {
            return picture;
        }
        let deadline = Instant::now() + TIMEOUT;
        while self.chat_scrolled_lately() {
            assert!(Instant::now() < deadline, "the chat kept scrolling, so its scrollbar never faded");
            std::thread::sleep(BETWEEN_PICTURES);
            self.frame();
        }
        self.settle()
    }

    /// Notes when the chat showing scrolled, as its scrollbar would: each chat's starts at the top, so one first
    /// drawn scrolled down to its latest row has scrolled, even by a point.
    pub(super) fn note_where_the_chat_is(&mut self) {
        let view = self.view.clone();
        let showing = self.cx.update(|cx| {
            let transcript = selected_pane(view.read(cx))?.read(cx).transcript()?;
            let offset = transcript.read(cx).list().offset().y;
            Some((transcript.entity_id(), offset))
        });
        let (chat, offset) = showing.map_or((None, px(0.)), |(chat, offset)| (Some(chat), offset));
        let before = if chat == self.chat.chat { self.chat.offset } else { px(0.) };
        if offset != before {
            self.chat.changed = Some(Instant::now());
        }
        (self.chat.chat, self.chat.offset) = (chat, offset);
    }

    /// Whether the chat scrolled recently enough that its scrollbar may still show.
    fn chat_scrolled_lately(&self) -> bool {
        self.chat.changed.is_some_and(|changed| changed.elapsed() < LONGER_THAN_IT_SHOWS)
    }
}
