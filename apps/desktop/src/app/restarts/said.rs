//! What restarting the hub or the app, quitting, or finishing an update would lose or stop, as each says before it
//! goes ahead: what's typed and not sent, and the agents, setup scripts and terminals running on the hub.

use protocol::client::HubStatus;

/// "1 chat" or "2 chats", or `None` for none.
fn counted(n: usize, what: &str) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("1 {what}")),
        n => Some(format!("{n} {what}s")),
    }
}

/// What `going`, such as restarting the app, would lose, or `None` for nothing: what only the app has of `chats`, such
/// as an answer being typed, or what the hub hasn't had yet, and what's typed in `comments` on the diff. It doesn't say
/// which, since there can be several kinds.
pub(in super::super) fn unsent_note(chats: usize, comments: usize, going: &str) -> Option<String> {
    let unsent = match (counted(chats, "chat"), counted(comments, "comment")) {
        (None, None) => return None,
        (Some(both), None) | (None, Some(both)) => both,
        (Some(chats), Some(comments)) => format!("{chats} and {comments}"),
    };
    Some(format!("Something in {unsent} hasn't been sent, and {going} loses it."))
}

/// What's running on a hub, as "the 2 agents, 1 setup script and 1 terminal", or `None` for nothing. A terminal's
/// shell runs in its workspace's worker, as an agent does, so stopping one stops the other.
fn running(status: &HubStatus) -> Option<String> {
    let count = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
    let running = [
        (status.running_sessions, "agent"),
        (status.running_setups, "setup script"),
        (status.open_terminals, "terminal"),
    ];
    let running: Vec<String> = running.into_iter().filter_map(|(n, what)| counted(count(n), what)).collect();
    match running.as_slice() {
        [] => None,
        [one] => Some(format!("the {one}")),
        [rest @ .., last] => Some(format!("the {} and {last}", rest.join(", "))),
    }
}

/// That a stopped agent carries on, for after saying what stops.
fn picks_up(status: &HubStatus) -> &'static str {
    match status.running_sessions {
        0 => "",
        1 => " The agent picks up its conversation at its next prompt.",
        _ => " Each agent picks up its conversation at its next prompt.",
    }
}

/// What restarting the hub across a break would cost, since that screen can stop agents with one click.
pub(in super::super) fn restart_note(status: &HubStatus) -> String {
    let start = "Restarting it starts one that matches";
    match running(status) {
        None => format!("{start}. Nothing is running on it."),
        Some(running) => format!("{start}, and stops {running} running on it.{}", picks_up(status)),
    }
}

/// What the app asks before finishing an update by restarting the older hub. It only asks about a hub that's busy,
/// so one with nothing running has a request under way, which restarting cuts off too.
pub(super) fn finish_question(status: &HubStatus) -> String {
    match running(status) {
        None => "Nothing is running on the previous version, but something is still under way on it, such as \
                 creating or archiving a workspace. Restarting now cuts that off."
            .to_string(),
        Some(running) => format!("This stops {running} still running on the previous version.{}", picks_up(status)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarting_says_how_much_it_would_lose() {
        assert_eq!(unsent_note(0, 0, "restarting"), None);
        assert_eq!(
            unsent_note(1, 0, "restarting").unwrap(),
            "Something in 1 chat hasn't been sent, and restarting loses it."
        );
        assert_eq!(
            unsent_note(0, 1, "restarting").unwrap(),
            "Something in 1 comment hasn't been sent, and restarting loses it."
        );
        assert_eq!(
            unsent_note(2, 1, "restarting").unwrap(),
            "Something in 2 chats and 1 comment hasn't been sent, and restarting loses it."
        );
    }

    /// What a hub reports with these running on it, and no terminal open.
    fn running_on(agents: u32, setups: u32) -> HubStatus {
        HubStatus {
            pid: 1,
            hub_version: "older".to_string(),
            protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION,
            started_at_ms: 0,
            workers: agents,
            running_sessions: agents,
            clients: 1,
            on_demand: true,
            build_number: Some(1),
            running_setups: setups,
            requests: 1,
            keeping_awake: false,
            keeps_workers: false,
            oldest_protocol_version: None,
            oldest_running_worker_protocol: None,
            open_terminals: 0,
        }
    }

    #[test]
    fn the_restart_note_says_what_would_stop() {
        assert!(restart_note(&running_on(0, 0)).ends_with("Nothing is running on it."));
        assert!(restart_note(&running_on(1, 0)).contains("stops the 1 agent running on it. The agent picks up"));
        assert!(restart_note(&running_on(2, 0)).contains("stops the 2 agents running on it. Each agent picks up"));
        assert!(restart_note(&running_on(0, 2)).ends_with("stops the 2 setup scripts running on it."));
        assert!(restart_note(&running_on(2, 1)).contains("stops the 2 agents and 1 setup script running on it. Each"));
        let terminal = HubStatus { open_terminals: 1, ..running_on(0, 0) };
        assert!(restart_note(&terminal).ends_with("stops the 1 terminal running on it."));
        let all = HubStatus { open_terminals: 2, ..running_on(2, 1) };
        assert!(restart_note(&all).contains("stops the 2 agents, 1 setup script and 2 terminals running on it. Each"));
    }

    #[test]
    fn finishing_an_update_says_what_would_stop() {
        assert_eq!(
            finish_question(&running_on(0, 0)),
            "Nothing is running on the previous version, but something is still under way on it, such as creating \
             or archiving a workspace. Restarting now cuts that off."
        );
        assert_eq!(
            finish_question(&running_on(2, 1)),
            "This stops the 2 agents and 1 setup script still running on the previous version. Each agent picks up \
             its conversation at its next prompt."
        );
    }
}
