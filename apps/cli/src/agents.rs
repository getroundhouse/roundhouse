//! What each kind of agent the hub runs is called and offers, as its adapter describes it.

use client_core::{Agents, Client};
use protocol::capability::settings::{Effort, ModelOffer};
use protocol::model::{AgentDescription, AgentKind};

use crate::modes::mode_name;
use crate::session::effort_name;

/// Prints each kind the hub describes, a few lines each, with what a chat starts with marked.
pub(crate) fn print_agents(client: &Client) -> anyhow::Result<()> {
    let agents = Agents::read(client)?;
    if agents.described().is_empty() {
        anyhow::bail!("the hub doesn't describe its agents: it refused to, or its reply couldn't be read");
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
    let defaults = agent.defaults();
    let starts_on = |offer: &ModelOffer| Some(&offer.model) == defaults.model.as_ref();
    let models: Vec<String> = agent
        .models()
        .iter()
        .map(|offer| marked(starts_on(offer), &format!("{} ({})", offer.model.as_str(), offer.label)))
        .collect();
    // Those of the model a chat starts on.
    let at = |effort: &Effort| Some(effort) == defaults.effort.as_ref();
    let efforts: Vec<String> = defaults.model.as_ref().map_or_else(Vec::new, |model| {
        agent.efforts_for(model).iter().map(|effort| marked(at(effort), effort_name(effort))).collect()
    });
    let approval: Vec<String> =
        agent.modes().into_iter().map(|mode| marked(Some(mode) == defaults.mode, mode_name(mode))).collect();
    let compacts = if agent.compacts() { "  compacts when asked" } else { "" };
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
    use protocol::capability::settings::{ModelCatalog, Models};
    use protocol::fixtures::offer;
    use protocol::model::ModelName;

    use super::*;

    #[test]
    fn each_kind_says_what_a_chat_can_be_given_and_marks_what_it_starts_with() {
        let mut agent = protocol::fixtures::claude();
        let offered = vec![
            offer("opus", "Opus", &[Effort::High, Effort::Xhigh], Effort::Xhigh),
            offer("haiku", "Haiku", &[Effort::High], Effort::High),
        ];
        let catalog = ModelCatalog { offered, default_model: Some(ModelName::named("opus")) };
        agent.capabilities.settings.models = Some(Models::Static(catalog));

        let said = described(&agent, true);

        assert_eq!(
            said,
            "claude  new chats run it  compacts when asked\n  models: opus (Opus)*, haiku (Haiku)\n  efforts: high, \
             xhigh*\n  modes: bypassPermissions*, plan\n"
        );
    }
}
