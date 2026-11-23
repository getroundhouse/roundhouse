//! What the bottom of the sidebar says about which build is running, and where updating it has got to.

use client_core::Client;
use client_core::records::Snapshot;
use gpui_kit::App;
use protocol::event::AgentTask;
use protocol::model::{SessionStatus, UpdateAttempt};

use super::AppView;
use super::updating_hub::HubUpdateAsk;

impl AppView {
    pub fn build_line(&self, cx: &App) -> BuildLine {
        let client = self.client();
        let hub = client.as_ref().map(Client::hub_version);
        // As the hub counts them, from what the sidebar already has, so it's current without asking.
        let restarting = self.restartable_hub.is_some_and(|older| older.restarting);
        let chats = self.records.every_session().map(|s| (s.status, s.background_tasks.as_slice()));
        let terminals = self.records.workspaces().iter().map(|workspace| self.records.terminals(&workspace.id).len());
        let holding = Holding::of(restarting, chats, terminals.sum());
        let may_restart = self.restartable_hub.is_some();
        let ready = crate::updates::ready(cx);
        let (records, asked) = (&self.records, self.hub_update_asked.as_ref());
        BuildLine::new(self.hub.build_version, hub, may_restart, holding, ready, records, asked)
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
    /// The app has updated, and its hub is still the older build, `hub`, with work on it, such as `agents`, and
    /// `terminals` open that restarting it would end. The app restarts the hub once that work is done, or sooner if
    /// asked.
    Finishing { hub: String, agents: Agents, terminals: usize },
    /// The hub, `hub`, is installed as a service and has found `next`, a newer release, which it updates to when asked,
    /// or is updating to, as `updating` says. `problem` says why the last update to it didn't go ahead, if it didn't:
    /// why the hub refused the app's request, or why the update it took failed.
    Hub { hub: String, next: String, updating: bool, problem: Option<String> },
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

/// What holds up restarting an older hub: its agents on a turn, and the terminals restarting it would end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Holding {
    pub(super) agents: Agents,
    pub(super) terminals: usize,
}

impl Holding {
    /// What holds up an older hub with these chats and this many `terminals` open: nothing an app can count, on one
    /// `restarting`, which keeps its workers, so it stops none of its agents, ends no terminal, and waits only on a
    /// setup script. Otherwise the agents [`Agents::of`] counts, and every terminal, which restarting it ends, as on
    /// Windows, whose workers stop with their hub.
    pub(super) fn of<'a>(
        restarting: bool,
        chats: impl IntoIterator<Item = (SessionStatus, &'a [AgentTask])>,
        terminals: usize,
    ) -> Self {
        if restarting { Self::default() } else { Self { agents: Agents::of(chats), terminals } }
    }
}

impl BuildLine {
    /// `hub` is `None` while there's no connection. `may_restart` says whether the app may restart the hub it's
    /// connected to, `holding` what's on it that holds that up, `ready` is the version of an update that's downloaded,
    /// `records` the copy of the hub's records, which say what it found in the update feed, as one installed as a service
    /// reads it, and `asked` the app's own request to update it. A downloaded update comes first, since it's what installs, and the app's own update
    /// finishing next, since it's under way.
    pub(super) fn new(
        app: &str,
        hub: Option<&str>,
        may_restart: bool,
        holding: Holding,
        ready: Option<&str>,
        records: &Snapshot,
        asked: Option<&HubUpdateAsk>,
    ) -> Self {
        let another = hub.filter(|hub| *hub != app).map(str::to_string);
        let other_hub = another.clone().filter(|_| !may_restart);
        let update = match (ready, another) {
            (Some(next), _) => Some(Updating::Ready { next: next.to_string() }),
            (None, Some(hub)) if may_restart => {
                let Holding { agents, terminals } = holding;
                Some(Updating::Finishing { hub, agents, terminals })
            }
            (None, _) => hub.and_then(|hub| hub_update(hub, records, asked)),
        };
        Self { app: app.to_string(), other_hub, update }
    }
}

/// Where the update of `hub` has got to, as what it found in the feed says, in `records`, and the app's own request to
/// update it, while it found one. It's updating from when the app asks, until the hub says the update it took failed, or
/// refuses.
fn hub_update(hub: &str, records: &Snapshot, asked: Option<&HubUpdateAsk>) -> Option<Updating> {
    let found = records.hub_update()?;
    let under_way = match &found.attempt {
        Some(UpdateAttempt::UnderWay { to }) => Some(to.as_str()),
        Some(UpdateAttempt::Failed { .. } | UpdateAttempt::Unknown) | None => None,
    };
    let next = under_way.or(records.release_offered())?.to_string();
    let failed = match &found.attempt {
        Some(UpdateAttempt::Failed { to, why }) if *to == next => Some(why),
        Some(_) | None => None,
    };
    let (asking, refused) = match asked {
        // A failure the hub recorded before the app asked is of the update before.
        Some(HubUpdateAsk::Asked { next: asked, rev }) if *asked == next => {
            (failed.is_none() || !found.rev.supersedes(*rev), None)
        }
        Some(HubUpdateAsk::Refused { next: asked, why }) if *asked == next => (false, Some(why.clone())),
        Some(_) | None => (false, None),
    };
    let updating = under_way.is_some() || asking;
    let problem = if updating { None } else { refused.or_else(|| failed.cloned()) };
    Some(Updating::Hub { hub: hub.to_string(), next, updating, problem })
}

#[cfg(test)]
mod tests {
    use SessionStatus::{Background, Idle, Interrupted, Running, Unknown, WaitingApproval};
    use client_core::records::HubRecords;
    use protocol::event::TaskKind;
    use protocol::model::{HubUpdate, Record, Rev};

    use super::*;

    const NONE: Agents = Agents { working: 0, waiting_on_you: 0 };
    const NOTHING: Holding = Holding { agents: NONE, terminals: 0 };

    fn line(other_hub: Option<&str>, update: Option<Updating>) -> BuildLine {
        BuildLine { app: "abc".to_string(), other_hub: other_hub.map(str::to_string), update }
    }

    fn ready() -> Updating {
        Updating::Ready { next: "ghi".to_string() }
    }

    /// A case of the line for an app that's `abc`: the hub it's connected to, whether it may restart that hub, what
    /// holds that up, an update downloaded, and what the line says.
    type Case = (&'static str, Option<&'static str>, bool, Holding, Option<&'static str>, BuildLine);

    fn lines() -> [Case; 6] {
        let agents = Agents { working: 2, waiting_on_you: 1 };
        let held = Holding { agents, terminals: 3 };
        let finishing = line(None, Some(Updating::Finishing { hub: "def".to_string(), agents, terminals: 3 }));
        let under_the_card = line(Some("def"), Some(ready()));
        [
            ("the same build is only named once", Some("abc"), true, NOTHING, None, line(None, None)),
            ("with no connection there's nothing to compare", None, true, NOTHING, None, line(None, None)),
            ("an older hub it may restart is finishing the update", Some("def"), true, held, None, finishing),
            ("a hub it leaves alone is named", Some("def"), false, NOTHING, None, line(Some("def"), None)),
            ("a downloaded update comes first", Some("def"), true, NOTHING, Some("ghi"), line(None, Some(ready()))),
            // The card is over the build line, which still says which hub it is.
            ("a hub it leaves alone, and an update", Some("def"), false, NOTHING, Some("ghi"), under_the_card),
        ]
    }

    #[test]
    fn the_line_names_the_hub_only_when_its_another_the_app_leaves_alone_and_says_where_an_update_has_got_to() {
        for (name, hub, may_restart, holding, downloaded, want) in lines() {
            let records = HubRecords::default().snapshot();
            assert_eq!(BuildLine::new("abc", hub, may_restart, holding, downloaded, &records, None), want, "{name}");
        }
    }

    /// A hub installed as a service that has found a newer release, `version`, as the app is connected to it, with an
    /// update in `attempt`, at `rev`.
    fn found(version: Option<&str>, attempt: Option<UpdateAttempt>, rev: u64) -> HubUpdate {
        HubUpdate { version: version.map(str::to_string), looked_at_ms: 1, problem: None, attempt, rev: Rev::new(rev) }
    }

    fn hub(next: &str, updating: bool, problem: Option<&str>) -> Updating {
        let (hub, next, problem) = ("def".to_string(), next.to_string(), problem.map(str::to_string));
        Updating::Hub { hub, next, updating, problem }
    }

    /// A client's copy of the records of a hub that found `found`.
    fn holding(found: &HubUpdate) -> Snapshot {
        let mut held = HubRecords::default();
        held.apply_record(Record::HubUpdate(found.clone()));
        held.snapshot()
    }

    /// The line on an app connected to a hub `def` that found `found`, having asked as `asked`.
    fn line_with(downloaded: Option<&str>, found: &HubUpdate, asked: Option<&HubUpdateAsk>) -> BuildLine {
        BuildLine::new("abc", Some("def"), false, NOTHING, downloaded, &holding(found), asked)
    }

    /// The hub's update shows once nothing of the app's own is under way, and says when it's updating.
    #[test]
    fn a_hub_s_update_shows_after_the_app_s_own_and_says_when_it_s_updating() {
        let under_way = Some(UpdateAttempt::UnderWay { to: "ghi".to_string() });

        assert_eq!(
            line_with(None, &found(Some("ghi"), None, 1), None),
            line(Some("def"), Some(hub("ghi", false, None)))
        );
        assert_eq!(
            line_with(None, &found(Some("ghi"), under_way, 1), None),
            line(Some("def"), Some(hub("ghi", true, None)))
        );
        let ready = Some(Updating::Ready { next: "jkl".to_string() });
        assert_eq!(line_with(Some("jkl"), &found(Some("ghi"), None, 1), None), line(Some("def"), ready));
        assert_eq!(line_with(None, &found(None, None, 1), None), line(Some("def"), None), "nothing newer");
        let records = holding(&found(Some("ghi"), None, 1));
        let unconnected = BuildLine::new("abc", None, false, NOTHING, None, &records, None);
        assert_eq!(unconnected, line(None, None));
    }

    /// The app's request says it's updating, so it isn't sent twice, until the hub refuses it or says the update it
    /// took failed. A failure from before the app asked is of the update before.
    #[test]
    fn a_hub_s_update_the_app_asked_for_is_updating_until_it_s_refused_or_fails() {
        let asked = HubUpdateAsk::Asked { next: "ghi".to_string(), rev: Rev::new(2) };
        let refused = HubUpdateAsk::Refused { next: "ghi".to_string(), why: "busy".to_string() };
        let failed = || Some(UpdateAttempt::Failed { to: "ghi".to_string(), why: "offline".to_string() });

        let asking = line_with(None, &found(Some("ghi"), None, 2), Some(&asked));
        let failed_before = line_with(None, &found(Some("ghi"), failed(), 2), Some(&asked));
        let failed_since = line_with(None, &found(Some("ghi"), failed(), 3), Some(&asked));
        let never_asked = line_with(None, &found(Some("ghi"), failed(), 3), None);
        let was_refused = line_with(None, &found(Some("ghi"), None, 2), Some(&refused));
        let another_release = line_with(None, &found(Some("jkl"), None, 3), Some(&refused));

        assert_eq!(asking, line(Some("def"), Some(hub("ghi", true, None))));
        assert_eq!(failed_before, line(Some("def"), Some(hub("ghi", true, None))));
        assert_eq!(failed_since, line(Some("def"), Some(hub("ghi", false, Some("offline")))));
        assert_eq!(never_asked, line(Some("def"), Some(hub("ghi", false, Some("offline")))));
        assert_eq!(was_refused, line(Some("def"), Some(hub("ghi", false, Some("busy")))));
        assert_eq!(another_release, line(Some("def"), Some(hub("jkl", false, None))));
    }

    fn task(kind: TaskKind) -> AgentTask {
        AgentTask { id: "t".into(), description: None, kind, call: None, subagent: None }
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

    /// A hub restarting keeps its workers, so it stops none of its agents, ends no terminal, and waits only on a
    /// setup script. One that doesn't keep them ends both.
    #[test]
    fn an_update_waits_on_no_agent_or_terminal_of_an_older_hub_restarting() {
        let command = [task(TaskKind::Command)];
        let chats = [(Running, &[][..]), (WaitingApproval, &[][..]), (Background, &command[..])];
        assert_eq!(Holding::of(true, chats, 2), NOTHING);
        let agents = Agents { working: 2, waiting_on_you: 1 };
        assert_eq!(Holding::of(false, chats, 2), Holding { agents, terminals: 2 });
    }
}
