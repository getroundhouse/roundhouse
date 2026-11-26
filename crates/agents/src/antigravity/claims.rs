use protocol::capability::{
    Capabilities, Claimed, approvals, background, context, conversation, output, settings, turns,
};

/// It claims neither `tool_servers` nor `tool_server_reconnect`: `agy acp` says nothing of its servers, and tries one that
/// failed again by itself at each prompt, so it's never asked to.
pub(super) fn claims() -> Capabilities {
    let mut claims = Capabilities::default();
    claims.conversation.resume = Some(conversation::Resume { ids: conversation::ConversationIds::Chosen });
    claims.prompts.images = Some(Claimed {});
    claims.prompts.steer = Some(Claimed {});
    claims.output.stream_text = Some(Claimed {});
    claims.output.thinking = Some(output::Thinking { default: output::ThinkingForm::Summary, others: Vec::new() });
    claims.output.stream_thinking = Some(Claimed {});
    claims.calls.typed = Some(Claimed {});
    claims.calls.exit_codes = Some(Claimed {});
    claims.turns.cancel = Some(turns::Cancel { stops: turns::CancelStops::Nothing });
    claims.turns.tokens = Some(Claimed {});
    claims.turns.own_turns = Some(Claimed {});
    claims.background.background_commands = Some(background::BackgroundCommands { hears: background::Hears::AtOnce });
    claims.approvals.modes = Some(approvals::Modes { default: full_access(), others: vec![plan()] });
    claims.approvals.requests = Some(approvals::Requests {
        parts: vec![approvals::DecisionPart::Stop, approvals::DecisionPart::ForConversation],
        kept: Vec::new(),
    });
    claims.asking.plan_approval = Some(Claimed {});
    claims.asking.questions = Some(Claimed {});
    claims.settings.effort = Some(Claimed {});
    claims.settings.models = Some(super::description::models());
    claims.settings.live_changes = Some(settings::LiveChanges {
        model: Some(settings::Takes::NextTurn),
        mode: Some(settings::Takes::NextTurn),
        effort: Some(settings::Takes::NextTurn),
        ..Default::default()
    });
    claims.context.usage = Some(Claimed {});
    claims.context.compaction = Some(context::Compaction { keeping: context::Keeping::Ignored });
    claims
}

pub(super) fn full_access() -> approvals::ApprovalMode {
    approvals::ApprovalMode {
        unasked: approvals::Unasked::Everything,
        beyond: approvals::Beyond::Refused,
        sandbox: None,
        plan: false,
    }
}

pub(super) fn plan() -> approvals::ApprovalMode {
    approvals::ApprovalMode {
        unasked: approvals::Unasked::Reads,
        beyond: approvals::Beyond::Asks,
        sandbox: None,
        plan: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_claimed_capability_has_its_needs_met() {
        assert_eq!(claims().unmet_needs(), Vec::new());
    }
}
