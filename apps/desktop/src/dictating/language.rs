//! The language dictation hears, as the person chose it in Settings, kept in the app's settings by its code, such as
//! `"en"`, or `"auto"`.

use dictation::Language;
use serde::{Deserialize as _, Deserializer, Serializer};

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's `with` gives what it saves by reference")]
pub(crate) fn serialize<S: Serializer>(language: &Language, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(language.code())
}

/// What's kept, or Auto for anything else, such as a language a newer build hears and this one doesn't, since Auto
/// hears every language this one does.
pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Language, D::Error> {
    let saved = serde_json::Value::deserialize(deserializer)?;
    Ok(saved.as_str().and_then(Language::from_code).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use dictation::Language;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Kept(#[serde(with = "super")] Language);

    #[test]
    fn its_kept_by_code_and_one_this_build_doesnt_know_is_auto() {
        assert_eq!(serde_json::to_string(&Kept(Language::German)).unwrap(), r#""de""#);
        assert_eq!(serde_json::to_string(&Kept(Language::Auto)).unwrap(), r#""auto""#);
        let read = |text: &str| serde_json::from_str::<Kept>(text).unwrap().0;
        assert_eq!(read(r#""uk""#), Language::Ukrainian);
        assert_eq!(read(r#""tlh""#), Language::Auto);
        assert_eq!(read("7"), Language::Auto);
    }
}
