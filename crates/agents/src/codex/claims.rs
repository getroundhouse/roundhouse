use protocol::capability::{Capabilities, Claimed, approvals, context, conversation, settings, turns};

pub(super) fn claims() -> Capabilities {
    let mut claims = Capabilities::default();
    claims.conversation.resume = Some(conversation::Resume { ids: conversation::ConversationIds::Chosen });
    claims.prompts.images = Some(Claimed {});
    claims.prompts.steer = Some(Claimed {});
    claims.output.stream_text = Some(Claimed {});
    claims.output.thinking = Some(protocol::capability::output::Thinking {
        default: protocol::capability::output::ThinkingForm::Summary,
        others: Vec::new(),
    });
    claims.output.stream_thinking = Some(Claimed {});
    claims.output.todos = Some(Claimed {});
    claims.background.subagents = Some(Claimed {});
    claims.context.usage = Some(Claimed {});
    claims.context.auto_compaction = Some(Claimed {});
    // `thread/compact/start` takes nothing to keep.
    claims.context.compaction = Some(context::Compaction { keeping: context::Keeping::Ignored });
    claims.account.limits = Some(Claimed {});
    claims.setup.tool_servers = Some(Claimed {});
    // Not `tool_server_reload`, since its reload leaves a connected server as it is.
    claims.setup.tool_server_reconnect = Some(Claimed {});
    claims.turns.cancel = Some(turns::Cancel { stops: turns::CancelStops::Nothing });
    claims.approvals.modes = Some(approvals::Modes { default: full_access(), others: vec![plan()] });
    claims.approvals.requests = Some(approvals::Requests {
        parts: vec![approvals::DecisionPart::Stop, approvals::DecisionPart::ForConversation],
        kept: Vec::new(),
    });
    claims.asking.plan_approval = Some(Claimed {});
    claims.asking.questions = Some(Claimed {});
    claims.settings.effort = Some(Claimed {});
    claims.settings.models = Some(settings::Models::Discoverable);
    claims.settings.live_changes = Some(settings::LiveChanges {
        model: Some(settings::Takes::NextTurn),
        mode: Some(settings::Takes::NextTurn),
        effort: Some(settings::Takes::NextTurn),
        ..Default::default()
    });
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
        sandbox: Some(approvals::Sandbox::ReadOnly),
        plan: true,
    }
}
