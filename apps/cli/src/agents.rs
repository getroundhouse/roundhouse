//! What each kind of agent the hub runs is called and offers, as its adapter describes it.

use client_core::{Agents, Client};
use protocol::model::{AgentDescription, AgentFeature, AgentKind};

use crate::modes::mode_name;
use crate::session::effort_name;

/// Prints each kind the hub describes, a few lines each, with what a chat starts with marked.
pub(crate) fn print_agents(client: &Client) -> anyhow::Result<()> {
    let agents = Agents::read(client)?;
    if agents.described().is_empty() {
        anyhow::bail!("the hub doesn't describe its agents, which one older than this client can't");
    }
    let new_chats = agents.for_new_chat();
    for agent in agents.described().iter().filter(|agent| agent.kind != AgentKind::Unknown) {
        print!("{}", described(agent, agent.kind == new_chats));
    }
    Ok(())
}

/// One kind's lines, ending with a line break: its name on the wire, then what a chat can be given, each by the name the
/// command line takes. `*` marks what a chat starts with.
fn described(agent: &AgentDescription, new_chats: bool) -> String {
    let marked = |is: bool, name: &str| if is { format!("{name}*") } else { name.to_string() };
    let models: Vec<String> = agent
        .models
        .iter()
        .map(|choice| {
            marked(
                Some(&choice.model) == agent.defaults.model.as_ref(),
                &format!("{} ({})", choice.model.as_str(), choice.label),
            )
        })
        .collect();
    let efforts: Vec<String> = agent
        .efforts
        .iter()
        .map(|effort| marked(Some(effort) == agent.defaults.effort.as_ref(), effort_name(effort)))
        .collect();
    let default_mode = agent.default_mode();
    let approval: Vec<String> =
        agent.modes().into_iter().map(|mode| marked(Some(mode) == default_mode, mode_name(mode))).collect();
    let compacts = if agent.has(AgentFeature::Compacts) { "  compacts when asked" } else { "" };
    let new = if new_chats { "  new chats run it" } else { "" };
    if agent.discovers_models() {
        return format!(
            "{}{new}{compacts}\n  models and efforts are discovered for each project\n  modes: {}\n",
            agent.kind.as_str(),
            approval.join(", ")
        );
    }
    format!(
        "{}{new}{compacts}\n  models: {}\n  efforts: {}\n  modes: {}\n",
        agent.kind.as_str(),
        models.join(", "),
        efforts.join(", "),
        approval.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use protocol::capability::settings::Effort;
    use protocol::model::{ModelChoice, ModelName};

    use super::*;

    #[test]
    fn each_kind_says_what_a_chat_can_be_given_and_marks_what_it_starts_with() {
        let opus = ModelName::named("opus");
        let agent = AgentDescription {
            capabilities: Some(protocol::fixtures::claude_claims()),
            kind: AgentKind::Claude,
            name: "Claude".to_string(),
            usage_heading: "Claude usage".to_string(),
            models: vec![
                ModelChoice { model: opus.clone(), label: "Opus".to_string() },
                ModelChoice { model: ModelName::named("haiku"), label: "Haiku".to_string() },
            ],
            efforts: vec![Effort::High, Effort::Xhigh],
            defaults: protocol::capability::settings::Settings {
                model: Some(opus),
                effort: Some(Effort::Xhigh),
                ..Default::default()
            },
            features: vec![AgentFeature::Compacts],
            login: None,
        };

        let said = described(&agent, true);

        assert_eq!(
            said,
            "claude  new chats run it  compacts when asked\n  models: opus (Opus)*, haiku (Haiku)\n  efforts: high, \
             xhigh*\n  modes: bypassPermissions*, plan\n"
        );
    }
}
