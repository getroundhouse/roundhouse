use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
use protocol::event::QuestionAnswer;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ToolCallWire;
use super::updates::parse_tool_wire;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PermissionRequest {
    pub(crate) tool_call: ToolCallWire,
    pub(crate) options: Vec<PermissionOption>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PermissionOption {
    pub(crate) option_id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) kind: String,
}

pub(crate) fn parse_permission_request(params: &Value) -> Option<PermissionRequest> {
    let call_obj = params.get("toolCall")?.as_object()?;
    let tool_call = parse_tool_wire(call_obj)?;
    let options = params
        .get("options")
        .and_then(|v| serde_json::from_value::<Vec<PermissionOption>>(v.clone()).ok())
        .unwrap_or_default();
    Some(PermissionRequest { tool_call, options })
}

impl PermissionRequest {
    pub(crate) fn option_for(&self, always: bool, allow: bool) -> Option<&str> {
        let wanted = match (allow, always) {
            (true, true) => "allow_always",
            (true, false) => "allow_once",
            (false, true) => "reject_always",
            (false, false) => "reject_once",
        };
        self.options
            .iter()
            .find(|opt| opt.kind == wanted)
            .or_else(|| self.options.iter().find(|opt| opt.kind.starts_with(if allow { "allow" } else { "reject" })))
            .map(|opt| opt.option_id.as_str())
    }
}

pub(crate) fn permission_selected(option_id: &str) -> Value {
    json!({ "outcome": { "outcome": "selected", "optionId": option_id } })
}

pub(crate) fn permission_cancelled() -> Value {
    json!({ "outcome": { "outcome": "cancelled" } })
}

/// A parsed ACP `elicitation/create` form and the per-question schema keys needed to encode the person's reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElicitationForm {
    pub(crate) questions: Vec<Question>,
    pub(crate) fields: Vec<FieldSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldSpec {
    pub(crate) key: String,
    /// The sibling `<key>_other` field `agy` puts a typed answer in, beside a question with options.
    pub(crate) other_key: Option<String>,
    /// Values corresponding to each `QuestionOption` by 0-based index (e.g. `"1"`, `"2"` in `agy`).
    pub(crate) consts: Vec<String>,
    pub(crate) multi: bool,
}

pub(crate) fn parse_elicitation(params: &Value) -> Option<ElicitationForm> {
    let props = params.get("requestedSchema")?.get("properties")?.as_object()?;
    let mut numbered: Vec<(usize, String)> = props
        .keys()
        .filter(|key| !key.ends_with("_other"))
        .map(|key| {
            let order = key.strip_prefix('q').and_then(|n| n.parse::<usize>().ok()).unwrap_or(usize::MAX);
            (order, key.clone())
        })
        .collect();
    numbered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut questions = Vec::new();
    let mut fields = Vec::new();
    for (_, key) in numbered {
        let Some(prop) = props.get(&key).and_then(Value::as_object) else { continue };
        let other_name = format!("{key}_other");
        let has_other = props.contains_key(&other_name);
        let (choose, options, consts, multi) = choices_of(prop);
        let typed = if options.is_empty() || has_other { TypedAnswer::Yes } else { TypedAnswer::No };
        let text = prop
            .get("description")
            .or_else(|| prop.get("title"))
            .and_then(Value::as_str)
            .or_else(|| params.get("message").and_then(Value::as_str))
            .unwrap_or(&key)
            .to_owned();
        let header = prop.get("title").and_then(Value::as_str).filter(|t| *t != text).map(str::to_owned);
        questions.push(Question { text, header, options, choose, typed });
        fields.push(FieldSpec { key, other_key: has_other.then_some(other_name), consts, multi });
    }
    (!questions.is_empty()).then_some(ElicitationForm { questions, fields })
}

fn choices_of(prop: &Map<String, Value>) -> (Choose, Vec<QuestionOption>, Vec<String>, bool) {
    let multi_items = prop.get("items").and_then(|items| items.get("anyOf").or_else(|| items.get("oneOf")));
    let single_items = prop.get("oneOf").or_else(|| prop.get("anyOf"));
    let (multi, list) = match (multi_items.and_then(Value::as_array), single_items.and_then(Value::as_array)) {
        (Some(items), _) => (true, Some(items)),
        (None, Some(items)) => (false, Some(items)),
        (None, None) => (false, None),
    };
    let mut options = Vec::new();
    let mut consts = Vec::new();
    for entry in list.into_iter().flatten() {
        let Some(val) = entry.get("const").and_then(Value::as_str) else { continue };
        let label = entry.get("title").and_then(Value::as_str).unwrap_or(val).to_owned();
        let description = entry.get("description").and_then(Value::as_str).map(str::to_owned);
        options.push(QuestionOption { label, description, preview: None });
        consts.push(val.to_owned());
    }
    if multi { (Choose::Several, options, consts, true) } else { (Choose::One, options, consts, false) }
}

pub(crate) fn elicitation_accept(fields: &[FieldSpec], answers: &[QuestionAnswer]) -> Value {
    let mut content = Map::new();
    for (spec, answer) in fields.iter().zip(answers.iter()) {
        let mapped: Vec<String> = answer
            .picked
            .iter()
            .filter_map(|&idx| usize::try_from(idx).ok().and_then(|i| spec.consts.get(i)).cloned())
            .collect();
        if spec.multi {
            if !mapped.is_empty() {
                content.insert(spec.key.clone(), json!(mapped));
            }
        } else if let Some(first) = mapped.into_iter().next() {
            content.insert(spec.key.clone(), Value::String(first));
        }
        if let Some(typed) = answer.typed.as_deref().filter(|s| !s.is_empty()) {
            if let Some(other_key) = &spec.other_key {
                content.insert(other_key.clone(), Value::String(typed.to_owned()));
            } else if spec.consts.is_empty() {
                content.insert(spec.key.clone(), Value::String(typed.to_owned()));
            }
        }
    }
    if content.is_empty() {
        return json!({ "action": "decline" });
    }
    json!({ "action": "accept", "content": content })
}

pub(crate) fn elicitation_cancel() -> Value {
    json!({ "action": "cancel" })
}
