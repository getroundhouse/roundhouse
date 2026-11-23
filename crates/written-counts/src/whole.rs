use crate::Unsigned;

/// A count written whole: every figure of it, with a comma between each three, such as `1,212`.
pub fn whole(count: impl Unsigned) -> String {
    let figures = count.to_string();
    let mut written = String::with_capacity(figures.len() + figures.len() / 3);
    for (at, figure) in figures.chars().enumerate() {
        if at > 0 && (figures.len() - at).is_multiple_of(3) {
            written.push(',');
        }
        written.push(figure);
    }
    written
}
