//! A count of tokens, said the same on every card in the app and its transcript.

/// A count of tokens, to a tenth of a thousand or a million: `950`, `313.5k`, `1.0M`.
#[expect(clippy::cast_precision_loss, reason = "shown to one decimal place, which a count in the millions keeps")]
pub fn tokens(count: u64) -> String {
    match count {
        ..1_000 => count.to_string(),
        // Up to where a tenth of a thousand would round to a thousand thousands.
        1_000..999_950 => format!("{:.1}k", count as f64 / 1e3),
        _ => format!("{:.1}M", count as f64 / 1e6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_counted_in_thousands_and_millions_to_a_tenth() {
        assert_eq!(tokens(0), "0");
        assert_eq!(tokens(950), "950");
        assert_eq!(tokens(1_000), "1.0k");
        assert_eq!(tokens(313_500), "313.5k");
        assert_eq!(tokens(500_000), "500.0k");
        assert_eq!(tokens(999_949), "999.9k");
        assert_eq!(tokens(999_950), "1.0M");
        assert_eq!(tokens(1_000_000), "1.0M");
    }
}
