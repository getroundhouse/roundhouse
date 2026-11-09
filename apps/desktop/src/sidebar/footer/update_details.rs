//! The info mark in the update card's corner, and what it says on hover: which versions, and what happens next.

use std::time::Duration;

use gpui_kit::base::HoverCard;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, v_flex};
use gpui_kit::{
    Anchor, AnchoredPositionMode, AnyElement, App, Entity, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Pixels, Styled as _, TestSupportExt as _, anchored, deferred, div, point, px,
};

use crate::app::BuildLine;

/// As long as GPUI waits before any other tooltip in the app shows, so this one doesn't feel slower. They go as soon
/// as the pointer does, too.
const DELAY: Duration = Duration::from_millis(500);
/// Between the details and the top of the card they sit on.
const GAP: Pixels = px(6.);
/// What `border_1` draws around the card, outside where it's measured and where its corner is.
const BORDER: Pixels = px(1.);
/// Over everything else in the window, as a tooltip is.
const PRIORITY: usize = 1;

/// What the mark says.
pub(super) struct Details {
    /// The version it's updating from, and the one it's updating to.
    from: String,
    to: String,
    /// What that means for what's running, and what happens next.
    what_happens: String,
}

impl Details {
    /// `None` when there's no update to say anything about.
    pub(super) fn of(line: &BuildLine) -> Option<Self> {
        let (from, to, what_happens) = match line {
            BuildLine::UpdateReady { app, next } => (
                app,
                next,
                format!(
                    "Roundhouse {next} is downloaded, and this is {app}. Restarting installs it, and agents keep \
                     running. Quitting installs it too."
                ),
            ),
            BuildLine::Finishing { app, hub, agents: 0 } => (
                hub,
                app,
                format!(
                    "This is {app}, and the previous version, {hub}, is still busy, such as with a setup script or \
                     the command line connected to it. The update finishes by itself once it's free."
                ),
            ),
            BuildLine::Finishing { app, hub, agents: _ } => (
                hub,
                app,
                format!(
                    "This is {app}, and agents are still running on the previous version, {hub}. The update \
                     finishes by itself once they're done."
                ),
            ),
            BuildLine::Current { .. } | BuildLine::OtherHub { .. } => return None,
        };
        Some(Self { from: from.clone(), to: to.clone(), what_happens })
    }

    /// Muted, like the detail under the card's title, since it's for whoever wants to know more. Resting the pointer
    /// on it shows the details, which `shown` keeps.
    pub(super) fn mark(shown: &Entity<Shown>, cx: &App) -> AnyElement {
        let icon = Icon::new(IconName::Info).small().text_color(cx.theme().muted_foreground);
        let shown = shown.clone();
        // Only for when to show them. The kit keeps what a hover card shows clear of the window's edge by more than
        // the card is, so it couldn't line them up with the card.
        let hover =
            HoverCard::new("details").open_delay(DELAY).close_delay(Duration::ZERO).trigger(icon).on_open_change(
                move |open, _, cx| {
                    shown.update(cx, |shown, cx| {
                        shown.open = *open;
                        cx.notify();
                    });
                },
            );
        div()
            .id("update-details")
            .flex_none()
            // Level with the middle of the title beside it, whose line is taller than the mark.
            .mt(px(3.))
            .child(hover)
            .test_support()
            .into_any_element()
    }

    /// While they show: the versions first, as the heading, then what that means, just above the card and as wide
    /// as it, rather than wherever the pointer happens to be, so they read as the card's. Raised and rounded like the
    /// card, and wrapped to its width rather than in one long line across the window.
    pub(super) fn shown(&self, shown: &Shown, cx: &App) -> Option<AnyElement> {
        if !shown.open {
            return None;
        }
        let theme = cx.theme();
        let text = v_flex()
            .id("update-details-text")
            .gap_1()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(format!("{} → {}", self.from, self.to)))
            .child(div().text_xs().text_color(theme.muted_foreground).child(self.what_happens.clone()))
            .test_support();
        let details = div()
            .w(shown.card_width + BORDER * 2.)
            .px_3()
            .py_2p5()
            .rounded(px(10.))
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .child(text);
        let above = anchored()
            .position_mode(AnchoredPositionMode::Local)
            .position(point(-BORDER, -(BORDER + GAP)))
            .anchor(Anchor::BottomLeft)
            .child(details);
        // From the card's corner inside its border, which is on a whole point, as GPUI rounds where it puts them to.
        // Anywhere else in the card, such as inside its padding, could leave them half a point off its edges.
        let corner = div().absolute().top_0().left_0().child(deferred(above).with_priority(PRIORITY));
        Some(corner.into_any_element())
    }
}

/// Whether the details show, and how wide the card was last drawn, inside its border, which they match.
#[derive(Default)]
pub(super) struct Shown {
    open: bool,
    pub(super) card_width: Pixels,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(line: &BuildLine) -> Option<(String, String)> {
        Details::of(line).map(|details| (details.from, details.to))
    }

    #[test]
    fn a_downloaded_update_goes_from_this_build_to_the_one_downloaded() {
        let line = BuildLine::UpdateReady { app: "1.0.0".to_string(), next: "1.0.1".to_string() };

        assert_eq!(versions(&line), Some(("1.0.0".to_string(), "1.0.1".to_string())));
    }

    #[test]
    fn finishing_goes_from_the_hubs_build_to_the_apps() {
        for agents in [0, 2] {
            let line = BuildLine::Finishing { app: "1.0.0".to_string(), hub: "0.9.0".to_string(), agents };

            assert_eq!(versions(&line), Some(("0.9.0".to_string(), "1.0.0".to_string())), "{agents} agents");
        }
    }

    #[test]
    fn a_hub_with_nothing_to_finish_says_what_else_it_waits_on() {
        let line = BuildLine::Finishing { app: "1.0.0".to_string(), hub: "0.9.0".to_string(), agents: 0 };

        let details = Details::of(&line).unwrap();

        assert!(
            details.what_happens.contains("is still busy, such as with a setup script"),
            "{}",
            details.what_happens
        );
    }
}
