//! Every capability, by its area, and what a run claims of them all.

use serde::{Deserialize, Serialize};

use super::{
    account, approvals, asking, background, calls, context, conversation, output, prompts, settings, setup, turns,
};

/// Declares [`Capability`] and [`Capabilities`] from each area's module, so an area can't be named without its claims.
macro_rules! areas {
    ($($area:ident => $variant:ident,)+) => {
        /// One capability, by its area. It's only ever compared, so it has no form on the wire: what a run claims is
        /// its [`Capabilities`], and a client asks those whether it has one.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Capability {
            $(#[doc = concat!("One of [`", stringify!($area), "`]'s.")]
            $variant($area::Capability),)+
        }

        $(impl From<$area::Capability> for Capability {
            fn from(capability: $area::Capability) -> Self {
                Capability::$variant(capability)
            }
        })+

        /// What a run of an agent claims, an area at a time, as its adapter reports it, or what its kind claims, as
        /// its adapter describes the kind, until a run reports. So the hub and every client know what to offer and
        /// what to refuse without matching on the kind. A claim missing, from a hub before it, or in an area or of a
        /// capability this build doesn't know, is one the agent doesn't make.
        #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
        #[serde(default)]
        pub struct Capabilities {
            $(#[doc = concat!("What it claims of [`", stringify!($area), "`].")]
            pub $area: $area::Claims,)+
        }

        impl Capabilities {
            /// Whether it claims `capability`.
            pub fn has(&self, capability: impl Into<Capability>) -> bool {
                match capability.into() {
                    $(Capability::$variant(capability) => self.$area.has(capability),)+
                }
            }

            /// Whether it claims every one of `capabilities`.
            pub fn has_all(&self, capabilities: impl IntoIterator<Item = Capability>) -> bool {
                capabilities.into_iter().all(|capability| self.has(capability))
            }
        }
    };
}

areas! {
    conversation => Conversation,
    prompts => Prompts,
    output => Output,
    calls => Calls,
    approvals => Approvals,
    asking => Asking,
    settings => Settings,
    turns => Turns,
    context => Context,
    account => Account,
    background => Background,
    setup => Setup,
}

/// A claim that has nothing more to say than that the agent does it. It's an empty object on the wire.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Claimed {}
