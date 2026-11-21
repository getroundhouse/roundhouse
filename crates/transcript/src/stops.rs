//! Where `Option-Up` and `Option-Down` step a page that isn't a chat: a rendered Markdown file between its headings,
//! and a proposal between the parts of its outline. Each step puts what it stops at at the top, as pressing a part in a
//! proposal's outline does. A step back goes first to the top of what's at the top, if the page is scrolled into it,
//! and then to the stop before, as a step back in a chat goes first to the prompt being read.

use gpui_kit::{Pixels, px};

use crate::selection::Way;

/// The stop a step `way` goes to, of `stops`, the items of a page it stops at, in order, with the page's top `into`
/// this far into the item `top`.
pub(crate) fn stop_beside(stops: impl IntoIterator<Item = usize>, top: usize, into: Pixels, way: Way) -> Option<usize> {
    let mut stops = stops.into_iter();
    // Less than a pixel in is where a scroll to its top leaves it.
    let into = into >= px(1.);
    match way {
        Way::Back => stops.filter(|stop| *stop < top || (*stop == top && into)).last(),
        Way::On => stops.find(|stop| *stop > top),
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::stop_beside;
    use crate::selection::Way;

    #[test]
    fn a_step_goes_to_the_stop_before_or_after_the_top_and_back_to_the_top_of_one_scrolled_into() {
        let stops = [2, 5, 9];
        let cases = [
            ("on from one at the top, to the next", 5, px(0.), Way::On, Some(9)),
            ("on from between two, to the next", 6, px(0.), Way::On, Some(9)),
            ("on from the last, nowhere", 9, px(40.), Way::On, None),
            ("back from one at the top, to the one before", 5, px(0.), Way::Back, Some(2)),
            ("back from one scrolled into, to its top", 5, px(40.), Way::Back, Some(5)),
            ("back from less than a pixel into one, to the one before", 5, px(0.5), Way::Back, Some(2)),
            ("back from between two, to the one before", 7, px(0.), Way::Back, Some(5)),
            ("back from above the first, nowhere", 1, px(10.), Way::Back, None),
        ];

        for (name, top, into, way, want) in cases {
            assert_eq!(stop_beside(stops, top, into, way), want, "{name}");
        }
    }
}
