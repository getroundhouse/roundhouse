//! What the bottom of the sidebar says about which build is running, and where updating it has got to.

use client_core::Client;
use gpui_kit::App;
use protocol::event::AgentTask;
use protocol::model::SessionStatus;

use super::AppView;

impl AppView {
    pub fn build_line(&self, cx: &App) -> BuildLine {
        let client = self.client();
        let hub = client.as_ref().map(Client::hub_version);
        // As the hub counts them, from what the sidebar already has, so it's current without asking.
        let restarting = self.restartable_hub.is_some_and(|older| older.restarting);
        let chats = self.records.every_session().map(|s| (s.status, s.background_tasks.as_slice()));
        let agents = Agents::holding_up(restarting, chats);
        BuildLine::new(self.hub.build_version, hub, self.restartable_hub.is_some(), agents, crate::updates::ready(cx))
    }
}

/// Which build is running, and where an update has got to, which the sidebar draws apart: the build in its line, and
/// the update as a card over it. One build is all most people need to know about, so the hub only comes up when it
/// isn't the app's build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildLine {
    pub app: String,
    /// The hub's build, when it's another that the app leaves alone: one someone started themselves, or a newer one.
    pub other_hub: Option<String>,
    /// Where an update has got to, while there's something to say about it.
    pub update: Option<Updating>,
}

/// Where an update has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Updating {
    /// A newer build, `next`, is downloaded, which installs when the app restarts or quits.
    Ready { next: String },
    /// The app has updated, and its hub is still the older build, `hub`, with work on it, such as `agents`. The app
    /// restarts the hub once that work is done, or sooner if asked.
    Finishing { hub: String, agents: Agents },
}

/// The agents on a turn, which is what keeps an older hub from being replaced. Apart, because an update that waits
/// on an agent that's waiting on you never finishes by itself, and the card has to say so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Agents {
    /// Working, or with work left in the background, so they finish by themselves.
    pub working: usize,
    /// Waiting for an answer or an approval, so they only finish after someone gives it, whatever they left running
    /// meanwhile.
    pub waiting_on_you: usize,
}

impl Agents {
    /// The agents an update waits on, of these chats on an older hub: none, on one `restarting`, which stops none of
    /// them and waits only on a setup script, and otherwise the ones [`Agents::of`] counts.
    pub(super) fn holding_up<'a>(
        restarting: bool,
        chats: impl IntoIterator<Item = (SessionStatus, &'a [AgentTask])>,
    ) -> Self {
        if restarting { Self::default() } else { Self::of(chats) }
    }

    /// Counts the same chats the hub does, from each one's status and what it has in the background, as
    /// [`SessionStatus::counts_as_running`] says. One that only waits on its pull request isn't counted, since the next
    /// hub keeps the wait.
    pub(super) fn of<'a>(chats: impl IntoIterator<Item = (SessionStatus, &'a [AgentTask])>) -> Self {
        let mut agents = Self::default();
        for (status, _) in chats.into_iter().filter(|(status, background)| status.counts_as_running(background)) {
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
    /// A downloaded update comes first, since it's what installs.
    pub(super) fn new(app: &str, hub: Option<&str>, may_restart: bool, agents: Agents, ready: Option<&str>) -> Self {
        let another = hub.filter(|hub| *hub != app).map(str::to_string);
        let other_hub = another.clone().filter(|_| !may_restart);
        let update = match (ready, another) {
            (Some(next), _) => Some(Updating::Ready { next: next.to_string() }),
            (None, Some(hub)) if may_restart => Some(Updating::Finishing { hub, agents }),
            (None, _) => None,
        };
        Self { app: app.to_string(), other_hub, update }
    }
}

#[cfg(test)]
mod tests {
    use SessionStatus::{Background, Idle, Interrupted, Running, Unknown, WaitingApproval};
    use protocol::event::TaskKind;

    use super::*;

    const NONE: Agents = Agents { working: 0, waiting_on_you: 0 };

    fn line(other_hub: Option<&str>, update: Option<Updating>) -> BuildLine {
        BuildLine { app: "abc".to_string(), other_hub: other_hub.map(str::to_string), update }
    }

    fn ready() -> Updating {
        Updating::Ready { next: "ghi".to_string() }
    }

    #[test]
    fn the_same_build_is_only_named_once() {
        assert_eq!(BuildLine::new("abc", Some("abc"), true, NONE, None), line(None, None));
    }

    #[test]
    fn with_no_connection_there_is_nothing_to_compare() {
        assert_eq!(BuildLine::new("abc", None, true, NONE, None), line(None, None));
    }

    #[test]
    fn an_older_hub_the_app_may_restart_is_finishing_the_update() {
        let agents = Agents { working: 2, waiting_on_you: 1 };

        let built = BuildLine::new("abc", Some("def"), true, agents, None);

        assert_eq!(built, line(None, Some(Updating::Finishing { hub: "def".to_string(), agents })));
    }

    #[test]
    fn a_hub_the_app_leaves_alone_is_named() {
        assert_eq!(BuildLine::new("abc", Some("def"), false, NONE, None), line(Some("def"), None));
    }

    #[test]
    fn a_downloaded_update_comes_first() {
        assert_eq!(BuildLine::new("abc", Some("def"), true, NONE, Some("ghi")), line(None, Some(ready())));
    }

    /// The card is over the build line, which still says which hub it is.
    #[test]
    fn a_hub_the_app_leaves_alone_is_named_with_an_update_downloaded() {
        assert_eq!(BuildLine::new("abc", Some("def"), false, NONE, Some("ghi")), line(Some("def"), Some(ready())));
    }

    fn task(kind: TaskKind) -> AgentTask {
        AgentTask { id: "t".into(), description: None, kind, call: None }
    }

    /// An agent that asked something holds an update up as much as one that's working, and only you can move it on.
    #[test]
    fn an_agent_waiting_on_you_is_counted_apart_from_those_working() {
        let command = [task(TaskKind::Command)];
        let statuses = [Running, WaitingApproval, Background, Idle, WaitingApproval, Interrupted, Unknown];

        let agents = Agents::of(statuses.into_iter().map(|status| (status, &command[..])));

        assert_eq!(agents, Agents { working: 2, waiting_on_you: 2 });
    }

    /// The hub only goes idle, and the update only finishes, once none holds up its restart, so the card counts
    /// those. One that only waits on its pull request doesn't, since the next hub keeps the wait.
    #[test]
    fn the_agents_counted_are_the_ones_the_hub_counts_as_running() {
        let (command, wait) = ([task(TaskKind::Command)], [task(TaskKind::Wait)]);
        for status in [Running, WaitingApproval, Background, Idle, Interrupted, Unknown] {
            for background in [&command[..], &wait[..]] {
                let counted = Agents::of([(status, background)]) != NONE;
                assert_eq!(counted, status.counts_as_running(background), "{status:?} {background:?}");
            }
        }
        assert_eq!(Agents::of([(Background, &wait[..])]), NONE);
    }

    /// It stops none of them, and waits only on a setup script.
    #[test]
    fn an_update_waits_on_no_agent_of_an_older_hub_restarting() {
        let command = [task(TaskKind::Command)];
        let chats = [(Running, &[][..]), (WaitingApproval, &[][..]), (Background, &command[..])];
        assert_eq!(Agents::holding_up(true, chats), NONE);
        assert_eq!(Agents::holding_up(false, chats), Agents { working: 2, waiting_on_you: 1 });
    }
}
