//! Declaring an area's capabilities and its claims.

/// Declares an area's `Capability`, whose variants' docs are their contracts, and its `Claims`, with a claim of each,
/// so a capability can't be added without a place to claim it, or be claimed in another's place. `$what` names the
/// area in their docs.
macro_rules! area {
    ($what:literal; $($(#[$doc:meta])* $variant:ident => $field:ident: $claim:ty,)+) => {
        #[doc = concat!("What ", $what, " can do.")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Capability {
            $($(#[$doc])* $variant,)+
        }

        #[doc = concat!("What a run claims of ", $what, ": each capability's claim, or `None` where it isn't made.")]
        #[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq, Eq)]
        #[serde(default)]
        pub struct Claims {
            $(#[doc = concat!("The claim of [`Capability::", stringify!($variant), "`].")]
            #[serde(skip_serializing_if = "Option::is_none")]
            pub $field: Option<$claim>,)+
        }

        impl Claims {
            pub fn has(&self, capability: Capability) -> bool {
                match capability {
                    $(Capability::$variant => self.$field.is_some(),)+
                }
            }
        }
    };
}
pub(crate) use area;
