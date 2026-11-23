use std::fmt::Display;

/// What can be a count of things: an unsigned integer of any width, so a count of files as a `usize`, and one of
/// tokens the hub sends as a `u64`, are each written as they are.
pub trait Unsigned: Copy + Display + sealed::Sealed {}

pub(crate) mod sealed {
    /// Only the unsigned integers are counts, and each widens to the widest of them, which `shortened` divides.
    pub trait Sealed {
        fn widened(self) -> u128;
    }
}

macro_rules! unsigned {
    ($($integer:ty),*) => {
        $(
            impl sealed::Sealed for $integer {
                fn widened(self) -> u128 {
                    // No integer is wider than `u128`, so this never saturates.
                    u128::try_from(self).unwrap_or(u128::MAX)
                }
            }
            impl Unsigned for $integer {}
        )*
    };
}

unsigned!(u8, u16, u32, u64, u128, usize);
