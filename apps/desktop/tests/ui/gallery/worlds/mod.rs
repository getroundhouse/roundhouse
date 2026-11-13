//! What the hub holds for each screen in the gallery, built on [`World`].

#![cfg(test)]

mod accounts;
mod working;

pub(super) use accounts::{accounts, context_held_login, logins_unkept, none_found};
pub(super) use working::{
    queue_held, queued, steered, steered_thinking, streaming, thinking_streaming, waiting_on_checks, working_background,
};

mod scenarios;
pub(super) use scenarios::{
    archiving, attachments, chat_tabs, chat_world, closed_chats, context, context_capped, context_with_a_models_limit,
    context_without_limits, conversation, diff, finished_chat, finishing_update, in_plan_mode, own_hub, planning,
    read_image, setup_failed, sidebar, titled, unread, unread_titled,
};
