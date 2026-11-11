//! What the bottom of the sidebar says about which build is running, and about updating it.

use client_core::Client;
use gpui_kit::App;
use protocol::model::SessionStatus;

use super::AppView;

impl AppView {
    pub fn build_line(&self, cx: &App) -> BuildLine {
        let client = self.client();
        let hub = client.as_ref().map(Client::hub_version);
        // As the hub counts them, from what the sidebar already has, so it's current without asking.
        let agents = Agents::of(self.sessions.values().flatten().map(|s| s.status));
        BuildLine::new(self.hub.build_version, hub, self.restartable_hub.is_some(), agents, crate::updates::ready(cx))
    }
}

/// One build is all most people need to know about, so the hub only comes up when it isn't the app's build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildLine {
    /// The app and its hub are the same build, or there's no hub to compare it with.
    Current { app: String },
    /// A newer build is downloaded, which installs when the app restarts or quits.
    UpdateReady { app: String, next: String },
    /// The app has updated, and its hub is still the older build, with work on it, such as `agents`. The app restarts
    /// the hub once that work is done, or sooner if asked.
    Finishing { app: String, hub: String, agents: Agents },
    /// The hub is another build that the app leaves alone: one someone started themselves, or a newer one.
    OtherHub { app: String, hub: String },
}

/// The agents on a turn, which is what keeps an older hub from being replaced. Apart, because an update that waits
/// on an agent that's waiting on you never finishes by itself, and the card has to say so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Agents {
    /// Working, or with work left in the background, so they finish by themselves.
    pub working: usize,
    /// Waiting for an answer or an approval, so they only finish after someone gives it.
    pub waiting_on_you: usize,
}

impl Agents {
    /// Counts the same chats the hub does, which are those whose status [`SessionStatus::is_active`].
    pub(super) fn of(statuses: impl IntoIterator<Item = SessionStatus>) -> Self {
        let mut agents = Self::default();
        for status in statuses {
            match status {
                SessionStatus::Running | SessionStatus::Background => agents.working += 1,
                SessionStatus::WaitingApproval => agents.waiting_on_you += 1,
                SessionStatus::Idle | SessionStatus::Interrupted | SessionStatus::Unknown => {}
            }
        }
        agents
    }
}

impl BuildLine {
    /// `hub` is `None` while there's no connection. `may_restart` says whether the app may restart the hub it's
    /// connected to, `agents` which are on a turn on it, and `ready` is the version of an update that's downloaded.
    pub(super) fn new(app: &str, hub: Option<&str>, may_restart: bool, agents: Agents, ready: Option<&str>) -> Self {
        let app = app.to_string();
        if let Some(next) = ready {
            return Self::UpdateReady { app, next: next.to_string() };
        }
        match hub {
            Some(hub) if hub != app && may_restart => Self::Finishing { app, hub: hub.to_string(), agents },
            Some(hub) if hub != app => Self::OtherHub { app, hub: hub.to_string() },
            Some(_) | None => Self::Current { app },
        }
    }
}

#[cfg(test)]
mod tests {
    use SessionStatus::{Background, Idle, Interrupted, Running, Unknown, WaitingApproval};

    use super::*;

    const NONE: Agents = Agents { working: 0, waiting_on_you: 0 };

    #[test]
    fn the_same_build_is_only_named_once() {
        assert_eq!(BuildLine::new("abc", Some("abc"), true, NONE, None), BuildLine::Current { app: "abc".to_string() });
    }

    #[test]
    fn with_no_connection_there_is_nothing_to_compare() {
        assert_eq!(BuildLine::new("abc", None, true, NONE, None), BuildLine::Current { app: "abc".to_string() });
    }

    #[test]
    fn an_older_hub_the_app_may_restart_is_finishing_the_update() {
        let agents = Agents { working: 2, waiting_on_you: 1 };

        let line = BuildLine::new("abc", Some("def"), true, agents, None);

        assert_eq!(line, BuildLine::Finishing { app: "abc".to_string(), hub: "def".to_string(), agents });
    }

    #[test]
    fn a_hub_the_app_leaves_alone_is_named() {
        let line = BuildLine::new("abc", Some("def"), false, NONE, None);
        assert_eq!(line, BuildLine::OtherHub { app: "abc".to_string(), hub: "def".to_string() });
    }

    #[test]
    fn a_downloaded_update_comes_first() {
        let line = BuildLine::new("abc", Some("def"), true, NONE, Some("ghi"));
        assert_eq!(line, BuildLine::UpdateReady { app: "abc".to_string(), next: "ghi".to_string() });
    }

    /// An agent that asked something holds an update up as much as one that's working, and only you can move it on.
    #[test]
    fn an_agent_waiting_on_you_is_counted_apart_from_those_working() {
        let agents = Agents::of([Running, WaitingApproval, Background, Idle, WaitingApproval, Interrupted, Unknown]);

        assert_eq!(agents, Agents { working: 2, waiting_on_you: 2 });
    }

    /// The hub only goes idle, and the update only finishes, once none is active, so the card counts those.
    #[test]
    fn the_agents_counted_are_the_ones_the_hub_counts_as_running() {
        for status in [Running, WaitingApproval, Background, Idle, Interrupted, Unknown] {
            assert_eq!(Agents::of([status]) != NONE, status.is_active(), "{status:?}");
        }
    }
}
