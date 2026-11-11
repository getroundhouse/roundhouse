//! What a finished turn's card says in words and numbers.

use gpui_kit::App;
use protocol::model::{AgentKind, ModelName};

/// What answered: the models the agent's replies came from, in the order they first did, and the agent, as "Opus 5.5
/// via Claude Code". Only the agent, from a turn whose end doesn't say which models, and only the models with an agent
/// this build has no name for. `None` with neither.
pub(super) fn heading(models: &[ModelName], agent: AgentKind) -> Option<String> {
    let models = names(models);
    match (models, agent_name(agent)) {
        (Some(models), Some(agent)) => Some(format!("{models} via {agent}")),
        (models, agent) => models.or(agent.map(str::to_string)),
    }
}

/// The models by name, each once: "Opus 5.5", "Sonnet 5.5 and Haiku 5.5". `None` for none.
fn names(models: &[ModelName]) -> Option<String> {
    let mut names: Vec<String> = Vec::new();
    for name in models.iter().map(model_name) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let last = names.pop()?;
    Some(if names.is_empty() { last } else { format!("{} and {last}", names.join(", ")) })
}

/// What an agent is called. `None` for one this build doesn't know.
fn agent_name(agent: AgentKind) -> Option<&'static str> {
    match agent {
        AgentKind::Claude => Some("Claude Code"),
        AgentKind::Unknown => None,
    }
}

/// A model's name as people say it, from the ID its agent gives it: "Opus 5.5" for `claude-opus-5-5`, and
/// "Haiku 4.5" for `claude-haiku-4-5-20251001`, whose date says nothing a person needs. An ID in any other form is
/// shown as it is, since a guess at it could name the wrong model.
fn model_name(model: &ModelName) -> String {
    let id = model.as_str();
    said(id).unwrap_or_else(|| id.to_string())
}

/// `None` for an ID that isn't a family's name and then its version, a number or two at a time.
fn said(id: &str) -> Option<String> {
    let bare = id.strip_prefix("claude-").unwrap_or(id);
    // Such as `[1m]`, for a model with a larger context.
    let bare = bare.split_once('[').map_or(bare, |(before, _)| before);
    let mut parts = bare.split('-');
    let family = parts.next().filter(|family| !family.is_empty() && family.bytes().all(|b| b.is_ascii_lowercase()))?;
    let mut version: Vec<&str> = parts.collect();
    if version.last().is_some_and(|last| last.len() == 8 && is_digits(last)) {
        version.pop();
    }
    if !version.iter().all(|part| (1..=2).contains(&part.len()) && is_digits(part)) {
        return None;
    }
    let name = family[..1].to_ascii_uppercase() + &family[1..];
    Some(if version.is_empty() { name } else { format!("{name} {}", version.join(".")) })
}

fn is_digits(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit())
}

/// A moment as the card says it: its day, with its year, its time of day, and both together.
pub(super) struct Moment {
    date: String,
    time: String,
    whole: String,
}

impl Moment {
    /// `None` for a time too far off to say.
    pub(super) fn at(at_ms: i64, cx: &App) -> Option<Self> {
        let (date, time) = (look::date(at_ms, cx)?, look::time_of_day(at_ms, cx)?);
        Some(Moment { date, time, whole: look::date_and_time(at_ms, cx)? })
    }
}

/// When a turn started and ended, and how long it took.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Times {
    /// The day the turn was on, once, when it started and ended on one. Each row says its own otherwise.
    pub(super) day: Option<String>,
    /// Each name and what it says, in order: "Started", "Ended" and "Duration".
    pub(super) rows: Vec<(&'static str, String)>,
}

/// When the turn `started`, if its hub said, and `ended`, and how long it `took`, as its line says. On one day, the
/// day is said once and the rows say times, and across midnight each row says its whole date.
pub(super) fn times(started: Option<&Moment>, ended: &Moment, took: Option<String>) -> Times {
    let one_day = started.is_none_or(|started| started.date == ended.date);
    let said = |at: &Moment| if one_day { at.time.clone() } else { at.whole.clone() };
    let rows = started.map(|started| ("Started", said(started))).into_iter().chain([("Ended", said(ended))]);
    Times { day: one_day.then(|| ended.date.clone()), rows: rows.chain(took.map(|took| ("Duration", took))).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_is_named_as_people_say_it_and_an_id_in_another_form_as_it_is() {
        let name = |id| model_name(&ModelName::named(id));
        assert_eq!(name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(name("claude-fable-5-2"), "Fable 5.2");
        assert_eq!(name("claude-haiku-4-5-20251001"), "Haiku 4.5", "without its date");
        assert_eq!(name("claude-opus-5-5[1m]"), "Opus 5.5");
        assert_eq!(name("claude-opus-4"), "Opus 4");
        assert_eq!(name("opus"), "Opus", "an alias");
        assert_eq!(name("claude-3-5-sonnet-20241022"), "claude-3-5-sonnet-20241022");
        assert_eq!(name("fake-model"), "fake-model");
        assert_eq!(name("claude-opus-5-preview"), "claude-opus-5-preview");
    }

    #[test]
    fn the_heading_says_the_models_in_order_each_once_and_the_agent_they_answered_through() {
        let models = |ids: &[&'static str]| ids.iter().map(|id| ModelName::named(id)).collect::<Vec<_>>();
        let heading = |ids: &[&'static str], agent| heading(&models(ids), agent);

        assert_eq!(heading(&["claude-opus-5-5"], AgentKind::Claude).as_deref(), Some("Opus 5.5 via Claude Code"));
        assert_eq!(
            heading(&["claude-sonnet-5-5", "claude-haiku-5-5", "claude-opus-5-5"], AgentKind::Claude).as_deref(),
            Some("Sonnet 5.5, Haiku 5.5 and Opus 5.5 via Claude Code")
        );
        let one_twice = ["claude-opus-5-5", "claude-opus-5-5[1m]"];
        assert_eq!(heading(&one_twice, AgentKind::Claude).as_deref(), Some("Opus 5.5 via Claude Code"));
        assert_eq!(heading(&[], AgentKind::Claude).as_deref(), Some("Claude Code"), "a turn that doesn't say");
        assert_eq!(heading(&["claude-opus-5-5"], AgentKind::Unknown).as_deref(), Some("Opus 5.5"));
        assert_eq!(heading(&[], AgentKind::Unknown), None);
    }

    /// Said together as look says them.
    fn moment(date: &str, time: &str) -> Moment {
        Moment { date: date.to_string(), time: time.to_string(), whole: format!("{date}, {time}") }
    }

    fn rows(rows: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
        rows.iter().map(|(name, said)| (*name, (*said).to_string())).collect()
    }

    #[test]
    fn a_turn_on_one_day_says_the_day_once_and_its_times_in_rows() {
        let (started, ended) = (moment("Nov 1, 2026", "2:13 PM"), moment("Nov 1, 2026", "2:15 PM"));

        let times = times(Some(&started), &ended, Some("1m, 48s".to_string()));

        assert_eq!(times.day.as_deref(), Some("Nov 1, 2026"));
        assert_eq!(times.rows, rows(&[("Started", "2:13 PM"), ("Ended", "2:15 PM"), ("Duration", "1m, 48s")]));
    }

    #[test]
    fn a_turn_across_midnight_says_each_end_s_whole_date() {
        let (started, ended) = (moment("Oct 31, 2026", "11:57 PM"), moment("Nov 1, 2026", "12:04 AM"));

        let times = times(Some(&started), &ended, Some("7m, 0s".to_string()));

        assert_eq!(times.day, None);
        let said = [("Started", "Oct 31, 2026, 11:57 PM"), ("Ended", "Nov 1, 2026, 12:04 AM"), ("Duration", "7m, 0s")];
        assert_eq!(times.rows, rows(&said));
    }

    #[test]
    fn a_turn_from_a_hub_that_didnt_say_when_it_started_says_when_it_ended() {
        let times = times(None, &moment("Nov 1, 2026", "2:15 PM"), Some("1m, 40s".to_string()));
        assert_eq!(times.day.as_deref(), Some("Nov 1, 2026"));
        assert_eq!(times.rows, rows(&[("Ended", "2:15 PM"), ("Duration", "1m, 40s")]));

        let untimed = super::times(None, &moment("Nov 1, 2026", "2:15 PM"), None);
        assert_eq!(untimed.rows, rows(&[("Ended", "2:15 PM")]), "one too old to say how long it took either");
    }
}
