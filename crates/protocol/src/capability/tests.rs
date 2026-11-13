use serde_json::json;

use super::approvals::{Access, Asks, Covers, Lasts, Request, Rule, RuleEffect, Unasked};
use super::approvals::{AfterDenial, ApprovalMode, Beyond, Decision, DecisionPart, Kept, Modes, Remember, Requests};
use super::asking::PlanContext;
use super::asking::ServerAnswer;
use super::calls::CallKind;
use super::context::{Compaction, Keeping};
use super::conversation::{AgentPoint, PromptPoint, Start};
use super::output::ThinkingForm;
use super::prompts::Extras;
use super::settings::{Budget, BudgetUnit, Budgets, Effort, LiveChanges, Settings, Takes};
use super::settings::{PersonaName, Verbosity};
use super::*;
use crate::model::{AgentSessionId, ApprovalId, Seq, TaskId};
use crate::prompt::Prompt;

fn mode(unasked: Unasked, plan: bool) -> ApprovalMode {
    ApprovalMode { unasked, beyond: Beyond::Asks, sandbox: None, plan }
}

/// A run that asks before tools, taking an edited allow and a denial that stops, keeping rules for the
/// conversation, runs asking first or in plan mode, compacts with what to keep, and takes a change of mode at once.
fn claimed() -> Capabilities {
    Capabilities {
        approvals: approvals::Claims {
            modes: Some(Modes { default: mode(Unasked::Reads, false), others: vec![mode(Unasked::Reads, true)] }),
            requests: Some(Requests {
                parts: vec![DecisionPart::Edited, DecisionPart::Stop, DecisionPart::KeepAllow],
                kept: vec![Kept::Conversation],
            }),
            ..approvals::Claims::default()
        },
        asking: asking::Claims { plan_approval: Some(Claimed {}), ..asking::Claims::default() },
        context: context::Claims { compaction: Some(Compaction { keeping: Keeping::Read }), ..Default::default() },
        settings: settings::Claims {
            budgets: Some(Budgets { units: vec![BudgetUnit::Tokens] }),
            live_changes: Some(LiveChanges { mode: Some(Takes::Now), ..LiveChanges::default() }),
            ..settings::Claims::default()
        },
        ..Capabilities::default()
    }
}

#[test]
fn a_claim_is_an_object_and_one_null_missing_or_this_build_doesnt_know_claims_nothing() {
    let read: Capabilities = serde_json::from_value(json!({
        "prompts": {"steer": {}, "notes": null, "telepathy": {}},
        "turns": {"cancel": {"stops": "telepathically"}},
        "minds": {"reading": {}},
    }))
    .unwrap();

    assert!(read.has(prompts::Capability::Steer));
    assert!(!read.has(prompts::Capability::Notes) && !read.has(prompts::Capability::Asides));
    assert_eq!(read.turns.cancel.unwrap().stops, turns::CancelStops::Unknown);
    assert_eq!(serde_json::from_value::<Capabilities>(json!({})).unwrap(), Capabilities::default());
    assert_eq!(serde_json::to_value(&read.prompts).unwrap(), json!({"steer": {}}), "only what's claimed is sent");
}

fn point(prompt: u64) -> PromptPoint {
    PromptPoint { prompt: Seq::new(prompt), point: AgentPoint::from("p") }
}

#[test]
fn each_request_needs_the_capabilities_it_uses() {
    let prompt = |extras| Control::Prompt { seq: Seq::new(1), prompt: Prompt::new("hi"), extras };
    let task = TaskId::from("t");
    let needs = |control: Control| control.needs();

    assert_eq!(needs(prompt(Extras::default())), []);
    assert_eq!(
        needs(prompt(Extras { skill: Some("review".into()), context: Some("ws".into()), ..Extras::default() })),
        [Capability::from(prompts::Capability::Skills), prompts::Capability::HiddenContext.into()]
    );
    assert_eq!(needs(Control::Rewind { before: point(1) }), [conversation::Capability::Rewind.into()]);
    assert_eq!(needs(Control::RestoreFiles { since: point(1) }), [conversation::Capability::RestoreFiles.into()]);
    let preview = Control::PreviewRestoreFiles { since: point(1) };
    assert_eq!(needs(preview), [conversation::Capability::PreviewRestoreFiles.into()]);
    assert_eq!(needs(Control::StopTask { task: task.clone() }), [background::Capability::StopTasks.into()]);
    assert_eq!(needs(Control::ReadTaskOutput { task }), [background::Capability::TaskOutput.into()]);
    assert_eq!(needs(Control::ReadRules), [approvals::Capability::Rules.into()]);
    assert_eq!(needs(Control::Cancel), [turns::Capability::Cancel.into()]);
    assert_eq!(
        needs(Control::Change(Settings { effort: Some(Effort::High), ..Settings::default() })),
        [Capability::from(settings::Capability::Effort), settings::Capability::LiveChanges.into()]
    );
}

fn deny(words: Option<&str>, then: AfterDenial, keep: Option<Rule>) -> Decision {
    Decision::Deny { words: words.map(String::from), then, keep }
}

fn rule(kept: Kept) -> Rule {
    let covers = Covers::Calls { call: CallKind::Command };
    Rule { covers, matching: "npm test:*".into(), effect: RuleEffect::Allow, kept, described: None }
}

#[test]
fn a_decision_has_every_part_it_uses_and_needs_what_it_answers() {
    let allow = |edited: bool, remember| Decision::Allow { edited: edited.then_some(calls::Call::Advisor), remember };
    let parts = |decision: Decision| decision.parts();

    assert_eq!(parts(allow(false, Remember::Once)), []);
    assert_eq!(parts(allow(true, Remember::Conversation)), [DecisionPart::Edited, DecisionPart::ForConversation]);
    assert_eq!(parts(allow(true, Remember::Rule(rule(Kept::User)))), [DecisionPart::Edited, DecisionPart::KeepAllow]);
    assert_eq!(parts(deny(None, AfterDenial::Continue, None)), []);
    assert_eq!(parts(deny(Some("no"), AfterDenial::Stop, None)), [DecisionPart::Words, DecisionPart::Stop]);
    assert_eq!(parts(deny(None, AfterDenial::Continue, Some(rule(Kept::User)))), [DecisionPart::KeepDeny]);
    assert_eq!(Decision::Answer { answers: vec![] }.needs(), [asking::Capability::Questions.into()]);
    assert_eq!(
        Decision::ApprovePlan { then: mode(Unasked::Edits, false), context: PlanContext::Fresh }.needs(),
        [Capability::from(asking::Capability::PlanApproval), asking::Capability::FreshPlan.into()]
    );
}

#[test]
fn a_decision_is_sent_only_when_the_run_takes_every_part_and_rule_and_mode_it_has() {
    let decide = |decision| Control::Decide { approval: ApprovalId::from("a"), decision };
    let approve = |then, context| decide(Decision::ApprovePlan { then, context });
    let claims = claimed();

    assert!(!decide(deny(Some("no"), AfterDenial::Stop, None)).allowed(&claims), "words the run doesn't take");
    assert!(decide(deny(None, AfterDenial::Stop, None)).allowed(&claims));
    let keep = |kept| decide(Decision::Allow { edited: None, remember: Remember::Rule(rule(kept)) });
    assert!(keep(Kept::Conversation).allowed(&claims));
    assert!(!keep(Kept::Project).allowed(&claims), "a rule kept where the run keeps none");
    let refusing = Rule { effect: RuleEffect::Deny, ..rule(Kept::Conversation) };
    let keeps_refusing = decide(Decision::Allow { edited: None, remember: Remember::Rule(refusing) });
    assert!(!keeps_refusing.allowed(&claims), "an allow that keeps a rule that refuses");
    assert!(approve(mode(Unasked::Reads, false), PlanContext::Kept).allowed(&claims));
    assert!(!approve(mode(Unasked::Everything, false), PlanContext::Kept).allowed(&claims), "a mode not offered");
    assert!(!approve(mode(Unasked::Reads, true), PlanContext::Kept).allowed(&claims), "a plan mode, though offered");
    assert!(!approve(mode(Unasked::Reads, false), PlanContext::Fresh).allowed(&claims), "fresh isn't claimed");
    assert!(!decide(Decision::Answer { answers: vec![] }).allowed(&claims), "questions aren't claimed");
}

#[test]
fn a_change_reaches_a_running_agent_only_in_settings_it_takes_live_and_claims() {
    let change = |settings| Control::Change(settings).allowed(&claimed());
    let budget = |unit| Settings { budget: Some(Budget { most: 1, unit }), ..Settings::default() };

    assert!(change(Settings { mode: Some(mode(Unasked::Reads, true)), ..Settings::default() }));
    assert!(!change(Settings { mode: Some(mode(Unasked::Edits, false)), ..Settings::default() }), "not offered");
    assert!(!change(budget(BudgetUnit::Tokens)), "a budget the agent takes only at launch");
    assert!(budget(BudgetUnit::Tokens).allowed(&claimed()));
    assert!(!budget(BudgetUnit::MicroUsd).allowed(&claimed()), "a unit the kind doesn't count");
    assert!(!Settings { effort: Some(Effort::High), ..Settings::default() }.allowed(&claimed()));
    assert!(!Control::Change(Settings::default()).allowed(&Capabilities::default()), "nothing changes live");
}

#[test]
fn a_compaction_is_sent_something_to_keep_only_where_the_agent_reads_it() {
    let compact = |keeping: Option<&str>| Control::Compact { asked: Seq::new(1), keeping: keeping.map(String::from) };
    let ignores = Capabilities {
        context: context::Claims { compaction: Some(Compaction { keeping: Keeping::Ignored }), ..Default::default() },
        ..Capabilities::default()
    };

    assert!(compact(Some("the plan")).allowed(&claimed()));
    assert!(compact(None).allowed(&ignores));
    assert!(!compact(Some("the plan")).allowed(&ignores));
}

#[test]
fn a_launch_needs_what_it_starts_from() {
    let conversation = || AgentSessionId::from("c");

    assert_eq!(Start::New { conversation: None }.needs(), None);
    assert_eq!(Start::Resume { conversation: conversation() }.needs(), Some(conversation::Capability::Resume.into()));
    let fork = Start::Fork { from: conversation(), conversation: None, before: None };
    assert_eq!(fork.needs(), Some(conversation::Capability::Fork.into()));
    assert_eq!(Start::Import { conversation: conversation() }.needs(), Some(conversation::Capability::Import.into()));
}

#[test]
fn a_report_needs_what_it_reports_and_one_this_build_doesnt_know_needs_nothing_and_is_kept_nowhere() {
    let progress = |progress| Report::CallProgress { call: "t".into(), progress };
    let unknown: Report = serde_json::from_value(json!({"report": "telepathy", "thoughts": []})).unwrap();

    assert_eq!(progress(Progress::Input { json: "{".into() }).needs(), Some(calls::Capability::StreamInput.into()));
    assert_eq!(progress(Progress::Output { text: "ok".into() }).needs(), Some(calls::Capability::Progress.into()));
    assert_eq!(Report::FilesRestored { files: vec![] }.needs(), Some(conversation::Capability::RestoreFiles.into()));
    let would = Report::WouldRestoreFiles { paths: vec![], insertions: None, deletions: None };
    assert_eq!(would.needs(), Some(conversation::Capability::PreviewRestoreFiles.into()));
    assert_eq!(Report::FilesRestored { files: vec![] }.keeper(), Keeper::Log);
    assert_eq!(would.keeper(), Keeper::Answer);
    assert_eq!(Report::Limits { limits: vec![] }.keeper(), Keeper::Account);
    assert_eq!(unknown, Report::Unknown);
    assert_eq!((unknown.needs(), unknown.keeper()), (None, Keeper::Answer));
}

#[test]
fn each_other_request_needs_the_capability_it_uses() {
    let approval = || ApprovalId::from("a");
    let review = Control::Review { asked: Seq::new(1), target: turns::ReviewTarget::Uncommitted };
    let note = Control::Note { seq: Seq::new(1), prompt: Prompt::new("fyi") };
    let subagent = Extras { subagent: Some(PersonaName::from("Explore")), ..Extras::default() };

    assert_eq!(note.needs(), [prompts::Capability::Notes.into()]);
    assert_eq!(Control::Overrule { approval: approval() }.needs(), [approvals::Capability::Overrule.into()]);
    assert_eq!(review.needs(), [turns::Capability::Reviews.into()]);
    assert_eq!(subagent.needs(), [background::Capability::DirectSubagents.into()]);
    assert_eq!(
        Decision::Grant { access: vec![], lasts: Lasts::Turn }.needs(),
        [approvals::Capability::AccessRequests.into()]
    );
    assert_eq!(Decision::Respond(ServerAnswer::Decline).needs(), [asking::Capability::ServerRequests.into()]);
    assert_eq!(Decision::KeepPlanning { words: None }.needs(), [asking::Capability::PlanApproval.into()]);
}

#[test]
fn a_request_for_approval_needs_what_it_asks_and_what_comes_with_it() {
    let request = |asks, suggested: Vec<Rule>, always| Request { asks, suggested, always, why: None };
    let plan = || Asks::Plan { plan: "1. Fix it".into(), file: None };
    let network = Asks::Network { host: "crates.io".into(), protocol: approvals::NetworkProtocol::Https };

    assert_eq!(request(plan(), vec![], false).needs(), [asking::Capability::PlanApproval.into()]);
    assert_eq!(
        request(Asks::Call { call: calls::Call::Advisor }, vec![rule(Kept::User)], true).needs(),
        [
            Capability::from(approvals::Capability::Requests),
            approvals::Capability::SuggestedRules.into(),
            approvals::Capability::AlwaysAsks.into(),
        ]
    );
    assert_eq!(network.needs(), approvals::Capability::NetworkRequests.into());
    assert_eq!(Asks::Access { access: vec![] }.needs(), approvals::Capability::AccessRequests.into());
    assert_eq!(Asks::Questions { questions: vec![] }.needs(), asking::Capability::Questions.into());
}

#[test]
fn a_decision_with_something_this_build_doesnt_know_is_never_sent() {
    let decide = |decision| Control::Decide { approval: ApprovalId::from("a"), decision }.allowed(&claimed());
    let grant = |access| Decision::Grant { access, lasts: Lasts::Turn };
    let grants = Capabilities {
        approvals: approvals::Claims { access_requests: Some(Claimed {}), ..approvals::Claims::default() },
        ..claimed()
    };

    let unknown_mode = ApprovalMode { unasked: Unasked::Unknown, ..mode(Unasked::Reads, false) };
    let mut offers_unknown = claimed();
    offers_unknown.approvals.modes.as_mut().unwrap().others.push(unknown_mode);
    let edit = |edited| decide(Decision::Allow { edited: Some(edited), remember: Remember::Once });
    let lsp = |asks| calls::Call::Lsp(calls::call::Lsp { asks, path: "a.rs".into(), at: None });
    let keep = |rule| decide(Decision::Allow { edited: None, remember: Remember::Rule(rule) });

    assert!(!decide(Decision::ApprovePlan { then: mode(Unasked::Reads, false), context: PlanContext::Unknown }));
    let into_unknown = Decision::ApprovePlan { then: unknown_mode, context: PlanContext::Kept };
    assert!(!into_unknown.allowed(&offers_unknown), "a mode it doesn't know, though another it doesn't is offered");
    assert!(Decision::Grant { access: vec![Access::Network], lasts: Lasts::Turn }.allowed(&grants));
    assert!(!grant(vec![Access::Network, Access::Unknown]).allowed(&grants));
    assert!(edit(lsp(calls::call::LspQuestion::Hover)));
    assert!(!edit(calls::Call::Unknown), "a call it doesn't know");
    assert!(!edit(lsp(calls::call::LspQuestion::Unknown)), "a call that holds something it doesn't know");
    assert!(keep(rule(Kept::Conversation)));
    assert!(!keep(Rule { covers: Covers::Unknown, ..rule(Kept::Conversation) }), "a rule for what it doesn't know");
}

#[test]
fn a_rule_or_a_mode_holding_something_this_build_doesnt_know_isnt_known() {
    let known = rule(Kept::Conversation);
    let plain = mode(Unasked::Reads, false);

    assert!(known.is_known() && plain.is_known());
    for unknown in [
        Rule { covers: Covers::Calls { call: CallKind::Unknown }, ..known.clone() },
        Rule { covers: Covers::Unknown, ..known.clone() },
        Rule { effect: RuleEffect::Unknown, ..known.clone() },
        Rule { kept: Kept::Unknown, ..known.clone() },
    ] {
        assert!(!unknown.is_known(), "{unknown:?}");
    }
    for unknown in [
        ApprovalMode { unasked: Unasked::Unknown, ..plain },
        ApprovalMode { beyond: Beyond::Unknown, ..plain },
        ApprovalMode { sandbox: Some(approvals::Sandbox::Unknown), ..plain },
    ] {
        assert!(!unknown.is_known(), "{unknown:?}");
    }
}

#[test]
fn a_setting_this_build_doesnt_know_is_never_sent() {
    let unknown_mode = ApprovalMode { sandbox: Some(approvals::Sandbox::Unknown), ..mode(Unasked::Reads, false) };
    let mut claims = every_setting();
    claims.approvals.modes.as_mut().unwrap().others.push(unknown_mode);
    claims.output.thinking.as_mut().unwrap().others.push(ThinkingForm::Unknown);
    claims.settings.budgets.as_mut().unwrap().units.push(BudgetUnit::Unknown);
    let budget = Budget { most: 1, unit: BudgetUnit::Unknown };

    for settings in [
        Settings { mode: Some(unknown_mode), ..Settings::default() },
        Settings { thinking: Some(ThinkingForm::Unknown), ..Settings::default() },
        Settings { budget: Some(budget), ..Settings::default() },
        Settings { effort: Some(Effort::Unknown), ..Settings::default() },
        Settings { speed: Some(settings::Speed::Unknown), ..Settings::default() },
        Settings { verbosity: Some(Verbosity::Unknown), ..Settings::default() },
    ] {
        assert!(!settings.allowed(&claims), "{settings:?}");
    }
    assert!(Settings { verbosity: Some(Verbosity::Low), ..Settings::default() }.allowed(&claims));
}

#[test]
fn a_new_conversation_is_named_exactly_where_the_kind_takes_the_hubs_ids() {
    let ids = |ids| Capabilities {
        conversation: conversation::Claims {
            resume: Some(conversation::Resume { ids }),
            fork: Some(Claimed {}),
            ..Default::default()
        },
        ..Capabilities::default()
    };
    let new = |named: bool| Start::New { conversation: named.then(|| AgentSessionId::from("c")) };
    let fork = |named: bool| Start::Fork {
        from: AgentSessionId::from("b"),
        conversation: named.then(|| AgentSessionId::from("c")),
        before: None,
    };
    let given = ids(conversation::ConversationIds::Given);
    let chosen = ids(conversation::ConversationIds::Chosen);

    assert!(new(true).allowed(&given) && fork(true).allowed(&given));
    assert!(!new(false).allowed(&given) && !fork(false).allowed(&given), "the hub's ID left out");
    assert!(new(false).allowed(&chosen) && fork(false).allowed(&chosen));
    assert!(!new(true).allowed(&chosen) && !fork(true).allowed(&chosen), "an ID the agent would choose");
    assert!(new(false).allowed(&Capabilities::default()));
    assert!(!fork(false).allowed(&Capabilities::default()), "forking isn't claimed");
}

/// Where a run says whether it takes a change of one setting live.
type LiveSetting = fn(&mut LiveChanges) -> &mut Option<Takes>;

/// A run that claims every setting, and takes a change of none of them live.
fn every_setting() -> Capabilities {
    let mut claims = claimed();
    claims.settings.effort = Some(Claimed {});
    claims.settings.speed = Some(Claimed {});
    claims.settings.personas = Some(Claimed {});
    claims.settings.styles = Some(Claimed {});
    claims.settings.verbosity = Some(Claimed {});
    claims.settings.advisor = Some(Claimed {});
    claims.settings.live_changes = Some(LiveChanges::default());
    claims.output.thinking = Some(output::Thinking { default: ThinkingForm::Summary, others: vec![] });
    claims
}

#[test]
fn each_setting_reaches_a_running_agent_only_where_the_agent_takes_a_change_of_it() {
    let model = crate::model::ModelName::named("opus");
    let each: [(Settings, LiveSetting); 10] = [
        (Settings { model: Some(model.clone()), ..Settings::default() }, |live| &mut live.model),
        (Settings { mode: Some(mode(Unasked::Reads, true)), ..Settings::default() }, |live| &mut live.mode),
        (Settings { effort: Some(Effort::High), ..Settings::default() }, |live| &mut live.effort),
        (Settings { speed: Some(settings::Speed::Fast), ..Settings::default() }, |live| &mut live.speed),
        (Settings { thinking: Some(ThinkingForm::Summary), ..Settings::default() }, |live| &mut live.thinking),
        (Settings { persona: Some("build".into()), ..Settings::default() }, |live| &mut live.persona),
        (Settings { style: Some("terse".into()), ..Settings::default() }, |live| &mut live.style),
        (Settings { verbosity: Some(Verbosity::Low), ..Settings::default() }, |live| &mut live.verbosity),
        (Settings { advisor: Some(model.clone()), ..Settings::default() }, |live| &mut live.advisor),
        (Settings { budget: Some(Budget { most: 1, unit: BudgetUnit::Tokens }), ..Settings::default() }, |live| {
            &mut live.budget
        }),
    ];

    for (settings, takes) in each {
        let mut claims = every_setting();
        assert!(settings.allowed(&claims), "{settings:?} at a launch");
        assert!(!settings.allowed_live(&claims), "{settings:?} isn't taken live");
        *takes(claims.settings.live_changes.as_mut().unwrap()) = Some(Takes::NextTurn);
        assert!(settings.allowed_live(&claims), "{settings:?} is taken live");
    }
}

#[test]
fn a_setting_needs_its_capability_and_a_form_of_thinking_the_kind_offers() {
    let thinking = |form| Settings { thinking: Some(form), ..Settings::default() };

    assert_eq!(thinking(ThinkingForm::Full).needs(), [output::Capability::Thinking.into()]);
    assert_eq!(
        Settings { advisor: Some(crate::model::ModelName::named("opus")), ..Settings::default() }.needs(),
        [settings::Capability::Advisor.into()]
    );
    assert!(thinking(ThinkingForm::Summary).allowed(&every_setting()));
    assert!(!thinking(ThinkingForm::Full).allowed(&every_setting()), "a form the kind doesn't offer");
}

#[test]
fn a_reports_keeper_is_where_its_state_is_kept() {
    let aside = Report::Aside { asked: Seq::new(1), answer: "yes".into() };

    assert_eq!(Report::Findings { findings: vec![] }.needs(), Some(turns::Capability::ReviewFindings.into()));
    assert_eq!(Report::Models { models: vec![] }.keeper(), Keeper::Session);
    assert_eq!(aside.keeper(), Keeper::Log);
    assert_eq!(Report::ThinkingProgress { tokens: 1 }.keeper(), Keeper::Shown);
}
