//! What the CLI calls each level of effort: what `--effort` and `apply_flag_settings`'s `effortLevel` take, and what
//! `get_settings` reports as `applied.effort`. `docs/claude-cli.md` has how it behaves.

use protocol::model::Effort;
use serde_json::Value;

/// `None` for a level from a newer peer, which there's no name for here.
pub(super) fn level(effort: Effort) -> Option<&'static str> {
    match effort {
        Effort::Low => Some("low"),
        Effort::Medium => Some("medium"),
        Effort::High => Some("high"),
        Effort::Xhigh => Some("xhigh"),
        Effort::Max => Some("max"),
        Effort::Unknown => None,
    }
}

/// The level in use, from what `get_settings` answers. Not the settings' own `effortLevel`, which a level the CLI
/// doesn't know takes back to the user's, while the level in use stays as it was.
pub fn applied_effort(settings: &Value) -> Option<&str> {
    settings.pointer("/applied/effort").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_has_the_name_the_cli_gives_it() {
        let names: Vec<_> = Effort::ALL.into_iter().map(level).collect();

        assert_eq!(names, [Some("low"), Some("medium"), Some("high"), Some("xhigh"), Some("max")]);
        assert_eq!(level(Effort::Unknown), None);
    }
}
