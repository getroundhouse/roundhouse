//! Models by name, as people say them.

use protocol::model::ModelName;

/// A model's name as people say it, from the ID its agent gives it: "Opus 5.5" for `claude-opus-5-5`, and
/// "Haiku 4.5" for `claude-haiku-4-5-20251001`, whose date says nothing a person needs. An ID in any other form is
/// shown as it is, since a guess at it could name the wrong model.
pub(crate) fn model_name(model: &ModelName) -> String {
    let id = model.as_str();
    said(id).unwrap_or_else(|| id.to_string())
}

/// `None` for an ID that isn't a family's name and then its version, a number or two at a time.
fn said(id: &str) -> Option<String> {
    let bare = id.strip_prefix("claude-").unwrap_or(id);
    // Such as `[1m]`, for a model with a larger context.
    let bare = bare.split_once('[').map_or(bare, |(before, _)| before);
    let mut parts = bare.split('-');
    let family = parts.next().filter(|family| !family.is_empty() && family.bytes().all(|b| b.is_ascii_lowercase()))?;
    let mut version: Vec<&str> = parts.collect();
    if version.last().is_some_and(|last| last.len() == 8 && is_digits(last)) {
        version.pop();
    }
    if !version.iter().all(|part| (1..=2).contains(&part.len()) && is_digits(part)) {
        return None;
    }
    let name = family[..1].to_ascii_uppercase() + &family[1..];
    Some(if version.is_empty() { name } else { format!("{name} {}", version.join(".")) })
}

fn is_digits(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_is_named_as_people_say_it_and_an_id_in_another_form_as_it_is() {
        let name = |id| model_name(&ModelName::named(id));
        assert_eq!(name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(name("claude-fable-5-2"), "Fable 5.2");
        assert_eq!(name("claude-haiku-4-5-20251001"), "Haiku 4.5", "without its date");
        assert_eq!(name("claude-opus-5-5[1m]"), "Opus 5.5");
        assert_eq!(name("claude-opus-4"), "Opus 4");
        assert_eq!(name("opus"), "Opus", "an alias");
        assert_eq!(name("claude-3-5-sonnet-20241022"), "claude-3-5-sonnet-20241022");
        assert_eq!(name("fake-model"), "fake-model");
        assert_eq!(name("claude-opus-5-preview"), "claude-opus-5-preview");
    }
}
