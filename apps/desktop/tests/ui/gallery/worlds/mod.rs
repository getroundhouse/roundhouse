//! What the hub holds for each screen in the gallery, built on [`World`].

#![cfg(test)]

mod accounts;
mod github;
mod github_apps;
mod github_gh;
mod lines;
mod providers;
mod summarized;
mod working;

pub(super) use accounts::{
    accounts, automatic, context_held_login, context_in_reserve, context_placed_automatically, context_to_move,
    every_account_out, needs_an_account, none_found, reserves,
};
pub(super) use github::{
    ada, github_accounts, github_default_ended, github_no_default, github_signing_in, github_signing_in_to,
    github_unnamed, no_github_accounts,
};
pub(super) use github_apps::{
    github_app_installed, github_app_not_installed, github_app_not_looked, github_app_renewing,
};
pub(super) use github_gh::{
    github_gh_default, github_gh_logged_out, github_gh_offered, github_gh_other_user, github_gh_refused,
    github_gh_unread,
};
pub(super) use lines::with_lines;
pub(super) use providers::{
    providers_gh_logged_out, providers_hub_is_a_bot, providers_key_differs, providers_managed, providers_no_gh,
    providers_no_key, providers_ssh,
};
pub(super) use summarized::summarized;
pub(super) use working::{
    queue_held, queued, steered, steered_thinking, streaming, thinking_streaming, waiting_on_checks, waiting_on_stacks,
    working_background,
};

mod scenarios;
pub(super) use scenarios::{
    archiving, attachments, chat_tabs, chat_world, closed_chats, codex_images, context, context_capped,
    context_with_a_models_limit, context_without_limits, conversation, converse, diff, finished_chat, finishing_update,
    hub_update_found, in_plan_mode, made_image, own_hub, planning, read_image, setup_failed, sidebar, titled, unread,
    unread_titled, videos,
};
