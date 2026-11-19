use super::line::AROUND_THE_MIDDLE;
use crate::cards::CardKind;
use crate::colors::Colors;
use crate::space::Space;

/// A line on a card washed in a colour is in that colour, faintly, and on any other card in the edge's.
#[test]
fn a_line_on_a_washed_card_is_in_its_colour() {
    let colors = Colors::dark();

    assert_eq!(CardKind::Stopped.line(&colors), colors.stopped.opacity(0.2));
    assert_eq!(CardKind::NeedsYou.line(&colors), colors.needs_you.opacity(0.2));
    for kind in [CardKind::Over, CardKind::InPlace, CardKind::Chat] {
        assert_eq!(kind.line(&colors), colors.edge, "{kind:?}");
    }
}

/// What's in a line's middle, such as what a compaction did, is 12px from the line at each side, as the spec says.
#[test]
fn what_s_in_a_line_s_middle_is_12px_from_it() {
    assert_eq!(AROUND_THE_MIDDLE, Space::Medium);
}
