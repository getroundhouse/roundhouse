//! A count written out in words, as a person reads it.

/// `number` with a comma between each three digits.
pub fn thousands(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::thousands;

    #[test]
    fn a_count_has_a_comma_between_each_three_digits() {
        assert_eq!(
            [thousands(0), thousands(999), thousands(1_000), thousands(1_234_567)],
            ["0", "999", "1,000", "1,234,567"]
        );
    }
}
