//! Every shape of decision a run can be passed, so each adapter's tests hold it to taking what its claims let through.

use crate::capability::Capabilities;
use crate::capability::approvals::{AfterDenial, Asks, Covers, Decision, Lasts, Remember, Request, Rule, RuleEffect};
use crate::capability::asking::{PlanContext, QuestionAnswer, ServerAnswer, TypedAnswer};

/// Every shape of decision on `request` that the hub would pass on to a run that claims `claims`, since it answers what
/// was asked, as [`Decision::answers`] says, and the run takes it, as [`Decision::allowed`] says: each way to allow,
/// to refuse, in words or not, stopping the turn or not, keeping a rule in each place a rule can be kept, to answer, to
/// approve a plan, in its conversation or a fresh one, or keep to it, to grant access and to answer a tool server. An adapter's converter has
/// to take each one for a request the adapter made, or the hub records a decision the agent never hears.
pub fn passed_decisions(request: &Request, claims: &Capabilities) -> Vec<Decision> {
    let kept = claims.approvals.requests.as_ref().map_or_else(Vec::new, |requests| requests.kept.clone());
    let rule = |effect, kept| match request.suggested.first() {
        Some(suggested) => Rule { effect, kept, ..suggested.clone() },
        None => Rule { covers: Covers::Unknown, matching: "Bash(ls)".into(), effect, kept, described: None },
    };
    let mut decisions = vec![
        Decision::Allow { remember: Remember::Once },
        Decision::Allow { remember: Remember::Conversation },
        Decision::KeepPlanning { words: None },
        Decision::KeepPlanning { words: Some("Smaller".into()) },
        Decision::Respond(ServerAnswer::Decline),
        Decision::Respond(ServerAnswer::Cancel),
    ];
    decisions
        .extend(kept.iter().map(|&kept| Decision::Allow { remember: Remember::Rule(rule(RuleEffect::Allow, kept)) }));
    for words in [None, Some("Not that".to_string())] {
        for then in [AfterDenial::Continue, AfterDenial::Stop] {
            decisions.push(Decision::Deny { words: words.clone(), then, keep: None });
            let keeping = kept.iter().map(|&kept| Some(rule(RuleEffect::Deny, kept)));
            decisions.extend(keeping.map(|keep| Decision::Deny { words: words.clone(), then, keep }));
        }
    }
    decisions.extend([PlanContext::Kept, PlanContext::Fresh].map(|context| Decision::ApprovePlan { context }));
    match &request.asks {
        Asks::Questions { questions } => {
            let each = |answer: &dyn Fn(usize) -> QuestionAnswer| (0..questions.len()).map(answer).collect();
            decisions.push(Decision::Answer { answers: each(&|_| QuestionAnswer::default()) });
            decisions.push(Decision::Answer {
                answers: each(&|at| {
                    let picked = if questions[at].options.is_empty() { Vec::new() } else { vec![0] };
                    QuestionAnswer { picked, typed: None }
                }),
            });
            decisions.push(Decision::Answer {
                answers: each(&|at| {
                    let typed = questions[at].typed != TypedAnswer::No;
                    QuestionAnswer { picked: Vec::new(), typed: typed.then(|| "Teal".to_string()) }
                }),
            });
        }
        Asks::Access { access } => {
            for lasts in [Lasts::Turn, Lasts::Conversation] {
                decisions.push(Decision::Grant { access: access.clone(), lasts });
            }
        }
        Asks::Call { .. } | Asks::Network { .. } | Asks::Plan { .. } | Asks::Server(_) | Asks::Unknown => {}
    }
    decisions.retain(|decision| decision.answers(request) && decision.allowed(claims));
    decisions
}
