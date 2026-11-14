use protocol::capability::{Capabilities, Claimed, approvals, context, conversation, output, settings, turns};

pub(super) fn claims() -> Capabilities {
    let mut claims = Capabilities::default();
    claims.conversation.resume = Some(conversation::Resume { ids: conversation::ConversationIds::Given });
    claims.prompts.images = Some(Claimed {});
    claims.prompts.steer = Some(Claimed {});
    claims.output.stream_text = Some(Claimed {});
    claims.output.thinking = Some(output::Thinking { default: output::ThinkingForm::Summary, others: Vec::new() });
    claims.output.stream_thinking = Some(Claimed {});
    claims.turns.cancel = Some(turns::Cancel { stops: turns::CancelStops::Nothing });
    claims.turns.tokens = Some(Claimed {});
    claims.turns.cost = Some(Claimed {});
    let full = approvals::ApprovalMode {
        unasked: approvals::Unasked::Everything,
        beyond: approvals::Beyond::Asks,
        sandbox: None,
        plan: false,
    };
    let plan = approvals::ApprovalMode { unasked: approvals::Unasked::Reads, plan: true, ..full };
    claims.approvals.modes = Some(approvals::Modes { default: full, others: vec![plan] });
    claims.approvals.requests = Some(approvals::Requests {
        parts: vec![approvals::DecisionPart::Words, approvals::DecisionPart::Stop],
        kept: Vec::new(),
    });
    claims.approvals.always_asks = Some(Claimed {});
    claims.approvals.denials = Some(Claimed {});
    claims.asking.questions = Some(Claimed {});
    claims.asking.plan_approval = Some(Claimed {});
    claims.settings.effort = Some(Claimed {});
    claims.settings.models = Some(crate::model_offers::models(
        &super::description::MODELS,
        &super::description::EFFORTS,
        protocol::model::Effort::Xhigh,
    ));
    claims.settings.live_changes = Some(settings::LiveChanges {
        model: Some(settings::Takes::NextTurn),
        mode: Some(settings::Takes::Now),
        effort: Some(settings::Takes::NextTurn),
        ..Default::default()
    });
    claims.context.compaction = Some(context::Compaction { keeping: context::Keeping::Read });
    claims.context.auto_compaction = Some(Claimed {});
    claims.context.compaction_summary = Some(Claimed {});
    claims.context.compaction_tokens = Some(Claimed {});
    claims.context.compaction_progress = Some(Claimed {});
    claims.context.usage = Some(Claimed {});
    claims.account.limits = Some(Claimed {});
    claims.background.subagents = Some(Claimed {});
    claims.background.background_commands = Some(protocol::capability::background::BackgroundCommands {
        hears: protocol::capability::background::Hears::AtOnce,
    });
    claims.background.tool_tasks = Some(Claimed {});
    claims
}
