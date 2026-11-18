//! What Claude is called and offers, for the hub to tell clients.

use protocol::capability::settings::{Effort, Settings};
use protocol::model::{AgentDescription, AgentFeature, AgentKind, LoginDescription, ModelChoice, ModelName};

/// Aliases, so these stay the newest of each without a release here. There's no `default`, since a chat always says
/// which model it runs.
pub(super) const MODELS: [(ModelName, &str); 4] = [
    (ModelName::named("fable"), "Fable"),
    (ModelName::named("opus"), "Opus"),
    (ModelName::named("sonnet"), "Sonnet"),
    (ModelName::named("haiku"), "Haiku"),
];

pub(super) const EFFORTS: [Effort; 5] = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max];

/// What a chat runs with until another is chosen, in the mode its claims give first, full access. `docs/spec/chats.md`
/// has why it's Opus at Extra high. An alias, so it stays the newest of its kind without a release here.
fn defaults() -> Settings {
    Settings { model: Some(ModelName::named("opus")), effort: Some(Effort::Xhigh), ..Settings::default() }
}

/// The command that makes a token for a hub to hold, which `login` gives the CLI. An interactive terminal app, as
/// `docs/claude-cli.md` says, so the person runs it themselves.
const MAKES_A_TOKEN: &str = "claude setup-token";

/// How long a token `claude setup-token` makes works for, as the CLI says where it shows the token: "valid for 1
/// year".
const A_TOKEN_LASTS_MS: i64 = 365 * 24 * 60 * 60 * 1000;

pub fn description() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Claude,
        capabilities: Some(super::claims::claims()),
        name: "Claude Code".to_string(),
        usage_heading: "Claude usage".to_string(),
        models: Vec::from(MODELS.map(|(model, label)| ModelChoice { model, label: label.to_string() })),
        // Every level, since the CLI names each, as `effort` says.
        efforts: EFFORTS.to_vec(),
        defaults: defaults(),
        // It's asked with `/compact`, as `process` sends it.
        features: vec![AgentFeature::Compacts],
        login: Some(LoginDescription { made_with: MAKES_A_TOKEN.to_string(), lasts_ms: Some(A_TOKEN_LASTS_MS) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hub_can_hold_a_token_that_setup_token_makes_which_lasts_a_year() {
        let login = description().login.unwrap();

        assert_eq!(login.made_with, "claude setup-token");
        assert_eq!(login.lasts_ms, Some(31_536_000_000));
    }

    /// Listed least first, as a description lists them, so `max`, the most the CLI takes, comes last.
    #[test]
    fn every_level_offered_has_a_name_the_cli_takes_and_max_is_the_most() {
        let efforts = description().efforts;
        for effort in &efforts {
            assert!(crate::claude::effort::level(effort).is_some(), "{effort:?}");
        }
        assert_eq!(efforts.last(), Some(&Effort::Max), "{efforts:?}");
    }
}
