//! What the app says of a chat's prompt cache outside its transcript, and when that changes: a clock on its tab and
//! its workspace's row once it's about to expire, how long it has left on Home, a line above the composer once it has
//! expired, and a note on the other models in the picker while it's warm. Each says it only in a chat big enough for
//! writing the cache again to cost something, as [`Session::cache_worth_telling`] says. `docs/spec/chats.md` has the
//! rules.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div,
};
use look::CacheMark;
use protocol::model::{CacheState, Session, Workspace, WorkspaceStatus};

use crate::ago::{self, ago};

/// The longest the app waits to draw again what's said of a cache, however much later the next change is. A Mac's
/// timers don't count the time it sleeps, so one set for later than this could leave a clock showing for as long
/// after waking as the Mac slept.
const LONGEST_WAIT_MS: i64 = 5 * 60 * 1000;

/// How long the chat's cache has left at `now_ms`, once it's about to expire.
pub(crate) fn expiring_in_ms(session: &Session, now_ms: i64) -> Option<i64> {
    match session.cache_worth_telling()?.state(now_ms)? {
        CacheState::Expiring { left_ms } => Some(left_ms),
        CacheState::Warm { .. } | CacheState::Expired { .. } => None,
    }
}

/// Whether anything outside a transcript says what the caches of `workspace`'s chats hold: not for an archived one,
/// which nobody works in, or one on its way there or back, or still being made, or in a state this app doesn't know.
pub(crate) fn told_of(workspace: &Workspace) -> bool {
    match workspace.status {
        WorkspaceStatus::Ready => true,
        WorkspaceStatus::Creating
        | WorkspaceStatus::Archiving
        | WorkspaceStatus::Archived
        | WorkspaceStatus::Restoring
        | WorkspaceStatus::Unknown => false,
    }
}

/// The clock a tab or a row shows while a cache is about to expire, with `left_ms` left, which says so while it's
/// pointed at, and to assistive apps.
pub(crate) fn clock(id: impl Into<ElementId>, left_ms: i64, color: Hsla) -> impl IntoElement {
    let said = look::expires_in(left_ms);
    div()
        .id(id)
        .flex_none()
        .aria_label(said.clone())
        .tooltip(move |window, cx| Tooltip::new(said.clone()).build(window, cx))
        .child(look::cache_mark(CacheMark::Clock, color))
}

/// How long the soonest to expire of `sessions`' caches has left at `now_ms`, of those about to.
pub(crate) fn soonest_expiring<'a>(sessions: impl IntoIterator<Item = &'a Session>, now_ms: i64) -> Option<i64> {
    sessions.into_iter().filter_map(|session| expiring_in_ms(session, now_ms)).min()
}

/// The line above the composer once the chat's cache has expired, with what the next prompt writes again, which is
/// what the context holds.
pub(crate) fn expired_line(session: &Session, now_ms: i64) -> Option<String> {
    let cache = session.cache_worth_telling()?;
    let CacheState::Expired { .. } = cache.state(now_ms)? else { return None };
    let tokens = session.context.as_ref()?.used_tokens;
    Some(format!(
        "Cache expired {} · this prompt writes {} tokens again",
        ago(cache.expires_at_ms()?, now_ms),
        look::tokens(tokens)
    ))
}

/// What pointing at another model in the picker says while the chat's cache is warm, since each model has a cache of
/// its own. Once it has expired, switching costs no more than staying.
pub(crate) fn switching_note(session: &Session, now_ms: i64) -> Option<String> {
    let cache = session.cache_worth_telling()?;
    let (CacheState::Warm { .. } | CacheState::Expiring { .. }) = cache.state(now_ms)? else { return None };
    let tokens = session.context.as_ref()?.used_tokens;
    Some(format!("Switching writes this chat's {} tokens to the cache again", look::tokens(tokens)))
}

/// How long until what the tabs, the sidebar and Home say of the caches of `sessions` changes, from `now_ms`, so the
/// window can be drawn again then and not before: once one is about to expire, then as each minute goes, until it has.
pub(crate) fn next_change_in_ms<'a>(sessions: impl IntoIterator<Item = &'a Session>, now_ms: i64) -> Option<i64> {
    let each = sessions.into_iter().filter_map(|session| {
        let cache = session.cache_worth_telling()?;
        match cache.state(now_ms)? {
            CacheState::Warm { .. } => Some(cache.warns_at_ms()? - now_ms),
            CacheState::Expiring { left_ms } => look::next_minute_in_ms(left_ms),
            CacheState::Expired { .. } => None,
        }
    });
    each.min().map(capped)
}

/// How long until the line above `session`'s composer changes, from `now_ms`: once its cache expires, and from then
/// on, as what [`ago`] says of when it did changes.
pub(crate) fn composer_line_changes_in_ms(session: &Session, now_ms: i64) -> Option<i64> {
    let cache = session.cache_worth_telling()?;
    let expires_at_ms = cache.expires_at_ms()?;
    let wait_ms = match cache.state(now_ms)? {
        CacheState::Warm { left_ms } | CacheState::Expiring { left_ms } => left_ms,
        CacheState::Expired { .. } => ago::changes_in_ms(expires_at_ms, now_ms),
    };
    Some(capped(wait_ms))
}

/// A wait of at least a millisecond, so a change due now is drawn once rather than over and over, and at most
/// [`LONGEST_WAIT_MS`].
fn capped(wait_ms: i64) -> i64 {
    wait_ms.clamp(1, LONGEST_WAIT_MS)
}

#[cfg(test)]
mod tests;
