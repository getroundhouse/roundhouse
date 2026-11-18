//! What the hub holds for each screen in the gallery, built on [`World`].

#![cfg(test)]

mod accounts;
mod github;
mod github_apps;
mod working;

pub(super) use accounts::{
    accounts, automatic, context_held_login, context_in_reserve, context_placed_automatically, context_to_move,
    logins_unkept, none_found, reserves,
};
pub(super) use github::{
    ada, github_accounts, github_default_ended, github_signing_in, github_signing_in_to, no_github_accounts,
};
pub(super) use github_apps::{
    github_app_installed, github_app_not_installed, github_app_not_looked, github_app_renewing,
};
pub(super) use working::{
    queue_held, queued, steered, steered_thinking, streaming, thinking_streaming, waiting_on_checks, waiting_on_stacks,
    working_background,
};

mod scenarios;
pub(super) use scenarios::{
    archiving, attachments, chat_tabs, chat_world, closed_chats, codex_images, context, context_capped,
    context_with_a_models_limit, context_without_limits, conversation, converse, diff, finished_chat, finishing_update,
    in_plan_mode, made_image, own_hub, planning, read_image, setup_failed, sidebar, titled, unread, unread_titled,
};
