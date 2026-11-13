//! The info mark in the update card's corner, and what it says on hover: which versions, and what happens next.

use gpui_kit::component::{Icon, IconName, Sizable as _, v_flex};
use gpui_kit::{
    Anchor, AnchoredPositionMode, AnyElement, App, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Pixels, Styled as _, TestSupportExt as _, anchored, deferred, div, point, px,
};

use look::{InRegion as _, Region, Typeset as _};

use crate::app::{Agents, BuildLine, Updating};

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
        let app = &line.app;
        let (from, to, what_happens) = match line.update.as_ref()? {
            Updating::Ready { next } => (
                app,
                next,
                format!(
                    "Roundhouse {next} is downloaded, and this is {app}. Restarting installs it, and agents keep \
                     running. Quitting installs it too."
                ),
            ),
            Updating::Finishing { hub, agents } => (hub, app, finishing(app, hub, *agents)),
        };
        Some(Self { from: from.clone(), to: to.clone(), what_happens })
    }

    /// Muted, like the detail under the card's title, since it's for whoever wants to know more. Resting the pointer
    /// on it shows the details, which `shown` keeps.
    pub(super) fn mark(shown: &Entity<Shown>, cx: &App) -> AnyElement {
        let icon = Icon::new(IconName::Info).small().text_color(look::colors(cx).text_muted);
        let shown = shown.clone();
        // Only for when to show them, so it draws nothing, not even its own box. The kit keeps what a hover card shows
        // clear of the window's edge by more than the card is, so it couldn't line them up with the card.
        let hover = look::hover_card("details").trigger(icon).on_open_change(move |open, _, cx| {
            shown.update(cx, |shown, cx| {
                shown.open = *open;
                cx.notify();
            });
        });
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
        let colors = look::colors(cx);
        let text = v_flex()
            .gap_1()
            .child(div().typeset(look::Type::Body).emphasis().child(format!("{} → {}", self.from, self.to)))
            .child(div().typeset(look::Type::Detail).text_color(colors.text_muted).child(self.what_happens.clone()));
        let details = look::raised(cx)
            .region(Region::UpdateDetails)
            .w(shown.card_width + BORDER * 2.)
            .px_3()
            .py_2p5()
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

/// What finishing an update waits for. Agents waiting on you don't get done by themselves, so it says what does it.
fn finishing(app: &str, hub: &str, agents: Agents) -> String {
    let start = format!("You're on version {app} of the app, and");
    match (agents.working, agents.waiting_on_you) {
        (0, 0) => format!(
            "{start} a previous version, {hub}, is still busy, such as with a setup script or the command line \
             connected to it. The update finishes by itself once it's free."
        ),
        (_, 0) => format!(
            "{start} agents are still running on a previous version, {hub}. The update finishes by itself once \
             they're done."
        ),
        (0, _) => format!(
            "{start} agents on a previous version, {hub}, are waiting on you, for an answer or an approval. The \
             update finishes by itself once they have it and are done."
        ),
        (_, _) => format!(
            "{start} agents are still running on a previous version, {hub}. Some are waiting on you, for an answer \
             or an approval. The update finishes by itself once they're all done."
        ),
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
        let line = updating(Updating::Ready { next: "1.0.1".to_string() });

        assert_eq!(versions(&line), Some(("1.0.0".to_string(), "1.0.1".to_string())));
    }

    #[test]
    fn finishing_goes_from_the_hubs_build_to_the_apps() {
        for working in [0, 2] {
            let line = finishing_line(Agents { working, waiting_on_you: 0 });

            assert_eq!(versions(&line), Some(("0.9.0".to_string(), "1.0.0".to_string())), "{working} agents");
        }
    }

    #[test]
    fn a_hub_with_nothing_to_finish_says_what_else_it_waits_on() {
        let what_happens = what_happens(Agents::default());

        assert!(what_happens.contains("is still busy, such as with a setup script"), "{what_happens}");
    }

    /// Only the agents that work get done by themselves, so "once they're done" alone would be wrong for the rest.
    #[test]
    fn agents_waiting_on_you_are_said_to_be() {
        for (working, waiting_on_you) in [(0, 1), (2, 1)] {
            let what_happens = what_happens(Agents { working, waiting_on_you });

            assert!(what_happens.contains("waiting on you, for an answer or an approval"), "{what_happens}");
        }
        let what_happens = what_happens(Agents { working: 2, waiting_on_you: 0 });
        assert!(!what_happens.contains("waiting on you"), "{what_happens}");
    }

    fn updating(update: Updating) -> BuildLine {
        BuildLine { app: "1.0.0".to_string(), other_hub: None, update: Some(update) }
    }

    fn finishing_line(agents: Agents) -> BuildLine {
        updating(Updating::Finishing { hub: "0.9.0".to_string(), agents })
    }

    fn what_happens(agents: Agents) -> String {
        Details::of(&finishing_line(agents)).unwrap().what_happens
    }
}
