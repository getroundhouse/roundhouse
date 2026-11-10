//! How full an agent's context is.

use serde::{Deserialize, Serialize};

/// What an agent's context holds, as the agent last said. An agent says so as it works, so this is as of its newest
/// report, and stays true while it's idle or gone, since only a turn changes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ContextUsage {
    /// What's in the context now.
    pub used_tokens: u64,
    /// The most it holds.
    pub max_tokens: u64,
    /// What it's made of, with whatever is set aside or free, in the agent's own words and order. Some parts aren't
    /// counted in `used_tokens`, such as tools the agent loads only when it needs them, so they don't add up to it.
    #[serde(default)]
    pub parts: Vec<ContextPart>,
}

/// One part of a context, such as the conversation's messages or the room left.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ContextPart {
    /// What the agent calls it, for a person to read.
    pub name: String,
    pub tokens: u64,
    /// Whether it's in the context, and if not, why it's listed.
    #[serde(default)]
    pub kind: ContextPartKind,
}

/// What a part of a context is to it. Only what's used is counted in [`ContextUsage::used_tokens`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextPartKind {
    /// In the context now.
    Used,
    /// Not in the context, and loaded into it only when the agent needs it, as some tools are.
    Deferred,
    /// Room the agent keeps back, such as for shrinking the conversation once it's full.
    Buffer,
    /// Room nothing has taken.
    Free,
    /// A kind this build doesn't know, or one the agent didn't say.
    #[default]
    #[serde(other)]
    Unknown,
}

impl ContextUsage {
    /// How much of the context is used, from 0 to 1. Nothing, for a context that says it holds nothing.
    pub fn used_fraction(&self) -> f32 {
        fraction(self.used_tokens, self.max_tokens)
    }

    /// How much of the context `part` takes, from 0 to 1.
    pub fn fraction_of(&self, part: &ContextPart) -> f32 {
        fraction(part.tokens, self.max_tokens)
    }
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a share of a context, drawn to a tenth of a percent, which a token count in the millions keeps"
)]
fn fraction(tokens: u64, of: u64) -> f32 {
    if of == 0 {
        return 0.;
    }
    (tokens as f64 / of as f64).clamp(0., 1.) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(used_tokens: u64, max_tokens: u64) -> ContextUsage {
        ContextUsage { used_tokens, max_tokens, parts: Vec::new() }
    }

    #[test]
    fn a_share_is_of_the_most_the_context_holds() {
        let context = context(313_500, 500_000);
        let messages = ContextPart { name: "Messages".to_string(), tokens: 298_500, kind: ContextPartKind::Used };

        assert!((context.used_fraction() - 0.627).abs() < 1e-6);
        assert!((context.fraction_of(&messages) - 0.597).abs() < 1e-6);
    }

    #[test]
    fn a_context_that_holds_nothing_or_less_than_it_uses_still_has_a_share() {
        assert!(context(10, 0).used_fraction().abs() < f32::EPSILON);
        assert!((context(600, 500).used_fraction() - 1.).abs() < f32::EPSILON);
    }

    #[test]
    fn a_report_with_no_parts_still_reads_and_so_does_a_part_of_no_kind_or_a_newer_one() {
        let read: ContextUsage = serde_json::from_str(r#"{"used_tokens":5,"max_tokens":10}"#).unwrap();
        assert_eq!(read, context(5, 10));

        let parts = r#"[{"name":"a","tokens":1},{"name":"b","tokens":2,"kind":"borrowed"},{"name":"c","tokens":3,"kind":"free"}]"#;
        let parts: Vec<ContextPart> = serde_json::from_str(parts).unwrap();
        let kinds: Vec<_> = parts.iter().map(|part| part.kind).collect();
        assert_eq!(kinds, [ContextPartKind::Unknown, ContextPartKind::Unknown, ContextPartKind::Free]);
    }
}
