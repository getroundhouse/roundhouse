//! Stands in for the `option-ext` crate, written from its public API. The real one is MPL-2.0, and it
//! would ship in the app through `zed-font-kit`, then `dirs`, then `dirs-sys`. This repo keeps shipped
//! code free of copyleft, and these three small methods aren't worth an exception. `tools/policy` checks
//! that nothing like it comes back. Cargo uses this through `[patch.crates-io]` in the root manifest.

/// Extra operations on `Option`.
pub trait OptionExt<T> {
    /// Whether this holds a value equal to `x`.
    fn contains<U>(&self, x: &U) -> bool
    where
        U: PartialEq<T>;

    /// Like `map_or`, with the function first.
    fn map_or2<U, F: FnOnce(T) -> U>(self, f: F, default: U) -> U;

    /// Like `map_or_else`, with the function first.
    fn map_or_else2<U, F: FnOnce(T) -> U, D: FnOnce() -> U>(self, f: F, default: D) -> U;
}

impl<T> OptionExt<T> for Option<T> {
    fn contains<U>(&self, x: &U) -> bool
    where
        U: PartialEq<T>,
    {
        match self {
            Some(value) => x == value,
            None => false,
        }
    }

    fn map_or2<U, F: FnOnce(T) -> U>(self, f: F, default: U) -> U {
        self.map_or(default, f)
    }

    fn map_or_else2<U, F: FnOnce(T) -> U, D: FnOnce() -> U>(self, f: F, default: D) -> U {
        self.map_or_else(default, f)
    }
}

#[cfg(test)]
mod tests {
    use super::OptionExt;

    #[test]
    fn contains_compares_the_value() {
        assert!(Some(3).contains(&3));
        assert!(!Some(3).contains(&4));
        assert!(!None::<i32>.contains(&3));
    }

    #[test]
    fn the_map_helpers_take_the_function_first() {
        assert_eq!(Some(2).map_or2(|v| v * 2, 0), 4);
        assert_eq!(None::<i32>.map_or2(|v| v * 2, 7), 7);
        assert_eq!(Some(2).map_or_else2(|v| v * 2, || 0), 4);
        assert_eq!(None::<i32>.map_or_else2(|v| v * 2, || 9), 9);
    }
}
