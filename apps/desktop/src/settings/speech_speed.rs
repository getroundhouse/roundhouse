//! How fast the voice reads replies, as the person chose it in Settings, kept in the app's settings by its code, such
//! as `"faster"`.

use conversation::Speed;
use serde::{Deserialize as _, Deserializer, Serializer};

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's `with` gives what it saves by reference")]
pub(crate) fn serialize<S: Serializer>(speed: &Speed, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(speed.code())
}

/// What's kept, or Normal for anything else, such as a speed a newer build has and this one doesn't.
pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Speed, D::Error> {
    let saved = serde_json::Value::deserialize(deserializer)?;
    Ok(saved.as_str().and_then(Speed::from_code).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use conversation::Speed;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Kept(#[serde(with = "super")] Speed);

    #[test]
    fn its_kept_by_code_and_one_this_build_doesnt_know_is_normal() {
        assert_eq!(serde_json::to_string(&Kept(Speed::Fastest)).unwrap(), r#""fastest""#);
        let read = |text: &str| serde_json::from_str::<Kept>(text).unwrap().0;
        assert_eq!(read(r#""faster""#), Speed::Faster);
        assert_eq!(read(r#""ludicrous""#), Speed::Normal);
        assert_eq!(read("2"), Speed::Normal);
    }
}
