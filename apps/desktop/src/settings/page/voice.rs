//! Voice, the page for talking and listening: dictation, with the language it hears and how it quiets what's playing,
//! then speech, with how fast it reads. Only where `sound::HEARS`, since conversation mode needs both, so a device
//! that can't hear has no Voice page rather than an empty one.

use gpui_kit::assets::IconName as Lucide;

use super::{Body, Page, PageId};

pub(super) fn page() -> Option<Page> {
    if !sound::HEARS {
        return None;
    }
    let mut settings = vec![super::dictation_row::setting(), super::dictation_row::language()];
    let possible = sound::Quieting::ALL.into_iter().filter(|how| how.is_possible());
    settings.extend(possible.map(super::dictation_row::quieting));
    settings.extend([super::speech_row::setting(), super::speech_row::speed()]);
    Some(Page { id: PageId::Voice, name: "Voice", icon: Lucide::AudioLines, body: Body::Settings(settings) })
}
