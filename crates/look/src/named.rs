//! Declares a set of names once, such as the kinds of text or the corners, so the type and the list that the spec's
//! table and the gallery's sheet are checked against can't disagree about which names there are.

/// An enum of `$name`, documented by its first docs, with a variant for each name after it, documented by its own, and
/// `ALL`, every name in the order they're declared, which is the spec table's, for the tests and the sheets.
macro_rules! named {
    ($(#[doc = $doc:literal])+ $name:ident { $($(#[doc = $each:literal])+ $variant:ident,)+ }) => {
        $(#[doc = $doc])+
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[doc = $each])+ $variant,)+
        }

        impl $name {
            /// Every one, in the order they're declared, which is the spec table's.
            #[cfg(any(test, feature = "testing"))]
            pub const ALL: &[$name] = &[$($name::$variant,)+];
        }
    };
}

pub(crate) use named;
