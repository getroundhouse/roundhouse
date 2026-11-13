//! The info mark in the update card's corner, and what it says on hover: which versions, and what happens next.

use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, v_flex};
use gpui_kit::{
    Anchor, AnchoredPositionMode, AnyElement, App, Entity, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Pixels, Styled as _, TestSupportExt as _, anchored, deferred, div, point, px,
};

use look::{InRegion as _, Region};

use crate::app::{Agents, BuildLine};

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
            BuildLine::Finishing { app, hub, agents, terminals } => {
                (hub, app, finishing(app, hub, *agents, *terminals))
            }
            BuildLine::Current { .. } | BuildLine::OtherHub { .. } => return None,
        };
        Some(Self { from: from.clone(), to: to.clone(), what_happens })
    }

    /// Muted, like the detail under the card's title, since it's for whoever wants to know more. Resting the pointer
    /// on it shows the details, which `shown` keeps.
    pub(super) fn mark(shown: &Entity<Shown>, cx: &App) -> AnyElement {
        let icon = Icon::new(IconName::Info).small().text_color(cx.theme().muted_foreground);
        let shown = shown.clone();
        // Only for when to show them, so it draws nothing, not even its own box. The kit keeps what a hover card shows
        // clear of the window's edge by more than the card is, so it couldn't line them up with the card.
        let hover = look::hover_card("details").appearance(false).trigger(icon).on_open_change(move |open, _, cx| {
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
        let theme = cx.theme();
        let text = v_flex()
            .gap_1()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(format!("{} → {}", self.from, self.to)))
            .child(div().text_xs().text_color(theme.muted_foreground).child(self.what_happens.clone()));
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
fn finishing(app: &str, hub: &str, agents: Agents, terminals: usize) -> String {
    let start = format!("You're on version {app} of the app, and");
    let (open, its_open, closed) = match terminals {
        1 => ("a terminal open".to_string(), "its open terminal".to_string(), "that terminal is closed"),
        _ => (format!("{terminals} terminals open"), format!("its {terminals} open terminals"), "they're closed"),
    };
    let agents = match (agents.working, agents.waiting_on_you) {
        (0, 0) if terminals > 0 => {
            return format!(
                "{start} a previous version, {hub}, has {open}, which restarting it would end. The update finishes \
                 by itself once {closed}."
            );
        }
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
    };
    match terminals {
        0 => agents,
        1 => format!("{agents} It waits for {its_open} too, since restarting the hub would end it."),
        _ => format!("{agents} It waits for {its_open} too, since restarting the hub would end them."),
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

    /// A terminal never ends by itself, so the details say what to close.
    #[test]
    fn open_terminals_are_said_to_hold_it_up() {
        let line = |agents, terminals| BuildLine::Finishing {
            app: "1.0.0".to_string(),
            hub: "0.9.0".to_string(),
            agents,
            terminals,
        };
        let what_happens = |agents, terminals| Details::of(&line(agents, terminals)).unwrap().what_happens;
        let working = Agents { working: 1, waiting_on_you: 0 };

        let one = what_happens(Agents::default(), 1);
        let two = what_happens(Agents::default(), 2);
        let (one_with_agents, two_with_agents) = (what_happens(working, 1), what_happens(working, 2));

        assert!(one.contains("has a terminal open") && one.contains("once that terminal is closed"), "{one}");
        assert!(two.contains("has 2 terminals open") && two.contains("once they're closed"), "{two}");
        let (one_waits, two_waits) =
            ("its open terminal too, since restarting the hub would end it.", "its 2 open terminals");
        assert!(one_with_agents.ends_with(one_waits), "{one_with_agents}");
        assert!(two_with_agents.contains(two_waits) && two_with_agents.ends_with("end them."), "{two_with_agents}");
    }

    fn finishing_line(agents: Agents) -> BuildLine {
        BuildLine::Finishing { app: "1.0.0".to_string(), hub: "0.9.0".to_string(), agents, terminals: 0 }
    }

    fn what_happens(agents: Agents) -> String {
        Details::of(&finishing_line(agents)).unwrap().what_happens
    }
}
