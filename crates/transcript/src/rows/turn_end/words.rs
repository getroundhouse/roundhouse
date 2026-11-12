//! What a finished turn's card says in words and numbers.

use gpui_kit::App;
use protocol::model::ModelName;

use crate::model_names::model_name;

/// What answered: the models the agent's replies came from, in the order they first did, and the agent, as its
/// description calls it, as "Opus 5.5 via Claude Code". Only the agent, from a turn whose end doesn't say which models,
/// and only the models with an agent the hub hasn't described. `None` with neither.
pub(super) fn heading(models: &[ModelName], agent: Option<&str>) -> Option<String> {
    let models = names(models);
    match (models, agent) {
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
    fn the_heading_says_the_models_in_order_each_once_and_the_agent_they_answered_through() {
        let models = |ids: &[&'static str]| ids.iter().map(|id| ModelName::named(id)).collect::<Vec<_>>();
        let heading = |ids: &[&'static str], agent| heading(&models(ids), agent);

        assert_eq!(heading(&["claude-opus-5-5"], Some("Claude Code")).as_deref(), Some("Opus 5.5 via Claude Code"));
        assert_eq!(
            heading(&["claude-sonnet-5-5", "claude-haiku-5-5", "claude-opus-5-5"], Some("Claude Code")).as_deref(),
            Some("Sonnet 5.5, Haiku 5.5 and Opus 5.5 via Claude Code")
        );
        let one_twice = ["claude-opus-5-5", "claude-opus-5-5[1m]"];
        assert_eq!(heading(&one_twice, Some("Claude Code")).as_deref(), Some("Opus 5.5 via Claude Code"));
        assert_eq!(heading(&[], Some("Claude Code")).as_deref(), Some("Claude Code"), "a turn that doesn't say");
        assert_eq!(heading(&["claude-opus-5-5"], None).as_deref(), Some("Opus 5.5"));
        assert_eq!(heading(&[], None), None);
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
