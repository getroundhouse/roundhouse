use protocol::capability::{Capabilities, Claimed, approvals, context, conversation, output, settings, turns};
use protocol::model::CacheTtl;

pub(super) fn claims() -> Capabilities {
    let mut claims = Capabilities::default();
    claims.conversation.resume = Some(conversation::Resume { ids: conversation::ConversationIds::Given });
    claims.prompts.images = Some(Claimed {});
    claims.prompts.steer = Some(Claimed {});
    claims.output.stream_text = Some(Claimed {});
    claims.output.thinking = Some(output::Thinking { default: output::ThinkingForm::Summary, others: Vec::new() });
    claims.output.stream_thinking = Some(Claimed {});
    claims.output.cover_lines = Some(Claimed {});
    claims.calls.typed = Some(Claimed {});
    // The CLI says what an edit or a write changed beside its result, with the file's own line numbers. It says how a
    // command exited only when it failed, so it doesn't claim `exit_codes`.
    claims.calls.file_changes = Some(Claimed {});
    claims.turns.cancel = Some(turns::Cancel { stops: turns::CancelStops::Nothing });
    claims.turns.tokens = Some(Claimed {});
    claims.turns.cost = Some(Claimed {});
    claims.turns.own_turns = Some(Claimed {});
    claims.approvals.modes = Some(approvals::Modes { default: full_access(), others: vec![plan()] });
    claims.approvals.requests = Some(requests());
    claims.approvals.suggested_rules = Some(Claimed {});
    claims.approvals.always_asks = Some(Claimed {});
    claims.approvals.denials = Some(Claimed {});
    claims.asking.questions = Some(Claimed {});
    claims.asking.plan_approval = Some(Claimed {});
    claims.settings.effort = Some(Claimed {});
    // Every level, since the CLI names each, as `effort` says.
    claims.settings.models = Some(crate::model_offers::models(
        &super::description::MODELS,
        &super::description::EFFORTS,
        &super::description::STARTS_ON,
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
    claims.context.cache = Some(context::Cache {
        kind: context::CacheKind::Marked { ttls: vec![CacheTtl::FiveMinutes, CacheTtl::OneHour] },
    });
    claims.account.limits = Some(Claimed {});
    claims.setup.tool_servers = Some(Claimed {});
    claims.setup.tool_server_reconnect = Some(Claimed {});
    claims.setup.tool_server_reload = Some(Claimed {});
    claims.background.subagents = Some(Claimed {});
    claims.background.background_commands = Some(protocol::capability::background::BackgroundCommands {
        hears: protocol::capability::background::Hears::AtOnce,
    });
    claims.background.tool_tasks = Some(Claimed {});
    claims
}

/// What a decision on a request can say: a rule it suggests can be kept wherever the CLI keeps one, and a refusal can
/// say what to do instead, and stop the turn.
fn requests() -> approvals::Requests {
    approvals::Requests {
        parts: vec![approvals::DecisionPart::KeepAllow, approvals::DecisionPart::Words, approvals::DecisionPart::Stop],
        kept: vec![
            approvals::Kept::Conversation,
            approvals::Kept::Worktree,
            approvals::Kept::Project,
            approvals::Kept::User,
        ],
    }
}

/// Full access, the CLI's `bypassPermissions`: it does everything unasked, but for what it asks about in any mode.
pub(super) fn full_access() -> approvals::ApprovalMode {
    approvals::ApprovalMode {
        unasked: approvals::Unasked::Everything,
        beyond: approvals::Beyond::Asks,
        sandbox: None,
        plan: false,
    }
}

/// Plan mode, the CLI's `plan`: it reads unasked, and asks about the rest until its plan is approved.
pub(super) fn plan() -> approvals::ApprovalMode {
    approvals::ApprovalMode { unasked: approvals::Unasked::Reads, plan: true, ..full_access() }
}
