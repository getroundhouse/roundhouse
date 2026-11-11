//! Which model an agent runs, by the name its CLI takes.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// A model, by the name an agent's CLI takes for it: an alias such as `opus`, which stays the newest of its kind, or a
/// full ID. What a name means is the CLI's to say. It's never `default`, the Claude CLI's name for whatever it picks for
/// the account, so a session always says which model it runs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct ModelName(Cow<'static, str>);

/// Why a name names no model.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelNameError {
    #[error("a model's name can't be empty")]
    Empty,
    /// Every alias and ID the CLI takes is printable ASCII, with no spaces.
    #[error("a model's name is letters, digits and punctuation, with no spaces")]
    NotPrintable,
    #[error("`default` leaves the choice to the agent's CLI, so a model has to be named")]
    Default,
}

impl ModelName {
    /// A name written into this build. It's checked as it's built, so a constant that names no model doesn't compile.
    pub const fn named(name: &'static str) -> ModelName {
        assert!(check(name).is_ok(), "that names no model");
        ModelName(Cow::Borrowed(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ModelName {
    type Error = ModelNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        check(&name)?;
        Ok(ModelName(Cow::Owned(name)))
    }
}

impl From<ModelName> for String {
    fn from(name: ModelName) -> String {
        name.0.into_owned()
    }
}

/// Whether `name` names a model, as [`ModelName`] says. It's `const`, so [`ModelName::named`] can check a constant as
/// it compiles, which is why it goes byte by byte.
const fn check(name: &str) -> Result<(), ModelNameError> {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return Err(ModelNameError::Empty);
    }
    let mut at = 0;
    while at < bytes.len() {
        if !bytes[at].is_ascii_graphic() {
            return Err(ModelNameError::NotPrintable);
        }
        at += 1;
    }
    if matches!(bytes, b"default") { Err(ModelNameError::Default) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Result<ModelName, ModelNameError> {
        ModelName::try_from(name.to_string())
    }

    #[test]
    fn an_alias_or_a_full_id_names_a_model() {
        for name in ["opus", "haiku", "claude-opus-5-5", "opus[1m]", "us.anthropic.claude-sonnet-5-5"] {
            assert_eq!(named(name).map(|model| model.as_str().to_string()), Ok(name.to_string()));
        }
    }

    #[test]
    fn a_name_that_would_leave_the_choice_to_the_cli_or_names_nothing_is_refused() {
        assert_eq!(named("default"), Err(ModelNameError::Default));
        assert_eq!(named(""), Err(ModelNameError::Empty));
        for name in [" ", "\t", "opus ", "op us", "opus\n", "ópus"] {
            assert_eq!(named(name), Err(ModelNameError::NotPrintable), "{name:?}");
        }
    }

    #[test]
    fn on_the_wire_its_the_name_and_a_name_that_names_no_model_doesnt_read() {
        assert_eq!(serde_json::to_string(&ModelName::named("opus")).unwrap(), r#""opus""#);
        assert_eq!(serde_json::from_str::<ModelName>(r#""sonnet""#).unwrap(), ModelName::named("sonnet"));
        for refused in [r#""default""#, r#""""#, "null"] {
            assert!(serde_json::from_str::<ModelName>(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_name_built_in_and_one_read_are_the_same_model() {
        assert_eq!(ModelName::named("opus"), named("opus").unwrap());
    }
}
