use std::fmt::Display;

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

/// What can be a count of things: an unsigned integer of any width, so a count of files as a `usize` and one the hub
/// sends as a `u64` are each written as they are.
pub trait Unsigned: Copy + Display + sealed::Sealed {}

mod sealed {
    pub trait Sealed {}
}

macro_rules! unsigned {
    ($($integer:ty),*) => {
        $(
            impl sealed::Sealed for $integer {}
            impl Unsigned for $integer {}
        )*
    };
}

unsigned!(u8, u16, u32, u64, u128, usize);
