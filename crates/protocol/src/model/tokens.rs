/// Declares token types: text that's all it takes to act as whoever it's for, so it's kept out of logs. Its `Debug`
/// says only that it's there, it has no `Display`, and a request or a reply that carries one can be logged whole. It's
/// a plain string on the wire, and in a file of the hub's. A crate that owns one declares it with `protocol::tokens!`, as
/// the hub does its own, which never cross to another process but GitHub.
#[macro_export]
macro_rules! tokens {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(::serde::Serialize, ::serde::Deserialize, Clone, PartialEq, Eq, Hash)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(token: impl Into<String>) -> Self {
                Self(token.into())
            }

            /// The token itself, for handing on or keeping, and nothing else.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    )+};
}
