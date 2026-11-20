//! Requests for approval and what closed them, typed, from the forms the hub recorded them in before both were typed.
//! Claude's adapter recorded a request by its tool, input and the CLI's suggestions, with what it asked in `asking`
//! once it said, which said only what kind of call one to use a tool made before that, and nothing before adapters said
//! what a request asks. Codex's and Antigravity's recorded a typed request, with those fields beside it, empty. The
//! hub recorded what became of each as its outcome, with the answers to questions by text or by position, and nothing
//! more of what the person decided.

use protocol::capability::approvals::{AfterDenial, Asks, Covers, Decision, Kept, Remember, Request, Rule, RuleEffect};
use protocol::capability::asking::{Choose, PlanContext, Question, QuestionAnswer, QuestionOption, TypedAnswer};
use protocol::capability::calls::Call;
use serde_json::Value;

use super::calls::{self, PLAN, QUESTIONS};

/// What a request recorded beside its typed request, or in place of one, before requests were typed.
const OLDER: [&str; 6] = ["tool_name", "input", "description", "suggestions", "asking", "reason"];

/// Types the request for approval `body`, as JSON, in place, and says whether it changed. One that holds a typed
/// request keeps it, and loses only the fields beside it.
pub(crate) fn type_request(body: &mut Value) -> bool {
    let typed = body.get("request").is_some_and(|request| !request.is_null());
    let request = (!typed).then(|| request(body));
    let Some(fields) = body.as_object_mut() else { return false };
    let mut changed = false;
    for older in OLDER {
        changed |= fields.remove(older).is_some();
    }
    if let Some(Ok(request)) = request.map(serde_json::to_value) {
        fields.insert("request".to_owned(), request);
        changed = true;
    }
    changed
}

/// What the request `body`, in the form Claude's adapter recorded it in, asked. None is said to be asked in every
/// mode, since whether one was went unrecorded.
fn request(body: &Value) -> Request {
    let tool = body.get("tool_name").and_then(Value::as_str).unwrap_or_default();
    let input = body.get("input").unwrap_or(&Value::Null);
    let asks = match body.get("asking") {
        None | Some(Value::Null) => asked(tool, input),
        Some(asking) => match asking.get("kind").and_then(Value::as_str) {
            Some("tool_use") => {
                let typed = asking.get("typed").and_then(|typed| serde_json::from_value(typed.clone()).ok());
                Asks::Call { call: typed.unwrap_or_else(|| calls::call(tool, input)) }
            }
            Some("questions") => Asks::Questions { questions: questions(asking, "questions", "text", "multiple") },
            Some("plan") => Asks::Plan { plan: text_of(asking, "plan").unwrap_or_default(), file: plan_file(input) },
            _ => Asks::Unknown,
        },
    };
    let suggested = suggested(body.get("suggestions"));
    let why = text_of(body, "reason").filter(|reason| !reason.trim().is_empty());
    Request { asks: described(asks, text_of(body, "description")), suggested, always: false, why }
}

/// `asks`, with what the request said a command it asks to run is for, `description`, as the command's own words,
/// where it has none. What it said of any other call, such as the name of a file to write, the call says already.
fn described(mut asks: Asks, description: Option<String>) -> Asks {
    if let Asks::Call { call: Call::Command(command) } = &mut asks
        && command.description.is_none()
    {
        command.description = description.filter(|said| !said.trim().is_empty());
    }
    asks
}

/// What a request to use `tool` with `input` asked, from before adapters said. Whatever was sent is taken: a question
/// without its text is left out, as is an option without its label, and a plan without its text is an empty one.
fn asked(tool: &str, input: &Value) -> Asks {
    match tool {
        QUESTIONS => Asks::Questions { questions: questions(input, "questions", "question", "multiSelect") },
        PLAN => Asks::Plan { plan: text_of(input, "plan").unwrap_or_default(), file: plan_file(input) },
        _ => Asks::Call { call: calls::call(tool, input) },
    }
}

fn plan_file(input: &Value) -> Option<String> {
    text_of(input, "planFilePath")
}

/// The questions `value` holds at `key`, each with its words at `text` and whether several can be chosen at `several`,
/// as the CLI's input has them and as the adapter said them. The CLI took any text as an answer.
fn questions(value: &Value, key: &str, text: &str, several: &str) -> Vec<Question> {
    let questions = value.get(key).and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    questions
        .iter()
        .filter_map(|question| {
            let options = question.get("options").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
            let choose = if question.get(several).and_then(Value::as_bool) == Some(true) {
                Choose::Several
            } else {
                Choose::One
            };
            Some(Question {
                text: text_of(question, text)?,
                header: text_of(question, "header"),
                options: options
                    .iter()
                    .filter_map(|option| {
                        let label = text_of(option, "label")?;
                        let (description, preview) = (text_of(option, "description"), text_of(option, "preview"));
                        Some(QuestionOption { label, description, preview })
                    })
                    .collect(),
                choose,
                typed: TypedAnswer::Yes,
            })
        })
        .collect()
}

/// The rules the CLI suggested keeping, as its `permission_suggestions` had them, in its own form, `Tool(content)`.
fn suggested(suggestions: Option<&Value>) -> Vec<Rule> {
    let suggestions = suggestions.and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    let rules = suggestions.iter().filter(|suggestion| suggestion["type"] == "addRules").flat_map(|suggestion| {
        let effect = match suggestion["behavior"].as_str() {
            Some("allow") => RuleEffect::Allow,
            Some("ask") => RuleEffect::Ask,
            Some("deny") => RuleEffect::Deny,
            _ => RuleEffect::Unknown,
        };
        let kept = match suggestion["destination"].as_str() {
            Some("session") => Kept::Conversation,
            Some("localSettings") => Kept::Worktree,
            Some("projectSettings") => Kept::Project,
            Some("userSettings") => Kept::User,
            _ => Kept::Unknown,
        };
        let rules = suggestion["rules"].as_array().map_or(&[][..], Vec::as_slice);
        rules.iter().filter_map(move |rule| {
            let tool = text_of(rule, "toolName")?;
            let covers = Covers::Calls { call: calls::kind(&tool) };
            let matching =
                text_of(rule, "ruleContent").map_or_else(|| tool.clone(), |content| format!("{tool}({content})"));
            Some(Rule { covers, matching, effect, kept, described: None })
        })
    });
    rules.collect()
}

/// Types the resolution `body`, as JSON, in place, if it's in the form from before decisions were recorded, as
/// [`decision`] reads it, and says whether it was.
pub(crate) fn type_resolution(body: &mut Value, asks: Option<&Asks>, said: Option<&str>) -> bool {
    if body.get("outcome").is_none() {
        return false;
    }
    let decision = decision(body, asks, said);
    let Some(fields) = body.as_object_mut() else { return false };
    for older in ["outcome", "answers", "answers_by_position"] {
        fields.remove(older);
    }
    if let Some(Ok(decision)) = decision.map(serde_json::to_value) {
        fields.insert("decision".to_owned(), decision);
    }
    true
}

/// The decision the resolution `body`, in the form from before decisions were recorded, stood for, of a request that
/// asked `asks`, where the call it asked to make was denied in the words `said`, or `None` where nobody decided, as for
/// one cancelled. A use allowed was allowed once, a denial let the agent carry on, a plan approved was carried out in
/// the conversation it was planned in, as no client could say otherwise, and answers by text are matched to the
/// options they name.
/// Questions denied were left unanswered, an empty answer to each, which is how a person chooses not to answer now.
/// *Unconfirmed.*
fn decision(body: &Value, asks: Option<&Asks>, said: Option<&str>) -> Option<Decision> {
    let allowed = match body.get("outcome").and_then(Value::as_str) {
        Some("allowed") => true,
        Some("denied") => false,
        Some("cancelled") => return None,
        _ => return Some(Decision::Unknown),
    };
    let words = said.map(str::to_string);
    Some(match (asks, allowed) {
        (Some(Asks::Questions { questions }), true) => Decision::Answer { answers: answers(body, questions) },
        (Some(Asks::Questions { questions }), false) => {
            Decision::Answer { answers: vec![QuestionAnswer::default(); questions.len()] }
        }
        (Some(Asks::Plan { .. }), true) => Decision::ApprovePlan { context: PlanContext::Kept },
        (Some(Asks::Plan { .. }), false) => Decision::KeepPlanning { words },
        (_, true) => Decision::Allow { remember: Remember::Once },
        (_, false) => Decision::Deny { words, then: AfterDenial::Continue, keep: None },
    })
}

/// The answers to `questions` that the resolution `body` kept, one for each, by position or by each question's text,
/// with any it kept none for left unanswered.
fn answers(body: &Value, questions: &[Question]) -> Vec<QuestionAnswer> {
    if let Some(Ok(mut answers)) =
        body.get("answers_by_position").map(|answers| serde_json::from_value::<Vec<QuestionAnswer>>(answers.clone()))
    {
        answers.resize_with(questions.len(), QuestionAnswer::default);
        return answers;
    }
    let by_text = body.get("answers");
    let said = |question: &Question| by_text?.get(&question.text)?.as_str();
    questions
        .iter()
        .map(|question| said(question).map_or_else(QuestionAnswer::default, |said| by_its_text(question, said)))
        .collect()
}

/// `said`, the answer to `question` by text, by position: the option it names, by its label, or the options it names,
/// as a client from before joined several with a comma and a space, where the question takes several, and otherwise
/// what was typed.
fn by_its_text(question: &Question, said: &str) -> QuestionAnswer {
    let at = |label: &str| question.options.iter().position(|option| option.label == label).map(position);
    let picked = match at(said) {
        Some(one) => Some(vec![one]),
        None if question.choose == Choose::Several => said.split(", ").map(at).collect(),
        None => None,
    };
    match picked {
        Some(picked) => QuestionAnswer { picked, typed: None },
        None => QuestionAnswer { picked: Vec::new(), typed: Some(said.to_string()) },
    }
}

fn position(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

fn text_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}
