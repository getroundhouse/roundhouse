//! Designs for the iPhone app: every screen it would have to do on a phone what the product does today, drawn from
//! the design system's parts at a handheld's sizes, for the owner to judge before any of it is built, as "How a
//! feature is designed" in `docs/spec/README.md` says. They're mocks: what a screen says is made up here, and only a
//! chat is the real transcript. Nothing checks them against an image, since none is in the spec until it's built.
//! `cargo test -p mobile-ios --test ui designs` draws them into `.context/designs/phone/`.

#![cfg(test)]

mod canvas;
mod changes;
mod chats;
mod drawn;
mod git_settings;
mod lists;
mod menus;
mod more;
mod parts;
mod settings;
mod worlds;
