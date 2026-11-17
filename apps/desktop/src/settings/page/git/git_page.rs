//! Git as Settings lists it, and the page as it's drawn: Added, then Found, in a column, each frame from the hub's
//! records and settings as client-core holds them, with what the page keeps from one frame to the next.

use client_core::records::Snapshot;
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::v_flex;
use gpui_kit::{App, Entity, IntoElement, ParentElement as _, RenderOnce, ScrollHandle, Styled as _, Window, px};
use protocol::model::{GithubAccount, GithubSettings, SentCredential};

use super::super::account_row::{COLUMN_WIDTH, Columns};
use super::kept::Kept;
use super::words::Standing;
use super::{Asking, Body, OfHub, Page, PageId, Within, added, found, words};

/// How wide a row's middle columns are: how it was connected, `Signed in` or `Token`, and what else there is to say,
/// such as when a token ends.
pub(super) const COLUMNS: Columns = Columns { how: px(140.), detail: px(200.) };

pub(in super::super) fn page(hub: &OfHub) -> Page {
    let (records, asking) = (hub.records.clone(), hub.asking.clone());
    let draw = Box::new(move |within: &Within| {
        GitPage { records: records.clone(), asking: asking.clone(), within: within.clone() }.into_any_element()
    });
    Page { id: PageId::Git, name: "Git", icon: Lucide::GitBranch, body: Body::Own { about: words::ABOUT, draw } }
}

/// The page, drawn from the hub's records and settings, with the hub to ask while there's one, within Settings.
#[derive(IntoElement)]
struct GitPage {
    records: Snapshot,
    asking: Option<Asking>,
    within: Within,
}

/// What every part of the page is drawn with.
pub(super) struct Drawing<'a> {
    pub(super) kept: &'a Entity<Kept>,
    pub(super) records: &'a Snapshot,
    /// The scroll of Settings' page, to bring a panel into sight as it opens.
    pub(super) scroll: &'a ScrollHandle,
    /// The one moment every row says a time as of.
    pub(super) now_ms: i64,
    /// What the hub's settings say of GitHub. `None` until they've arrived, and from a hub that keeps no GitHub
    /// accounts, which `older` tells apart.
    pub(super) github: Option<&'a GithubSettings>,
    /// The hub's settings have arrived, and say nothing of GitHub, since it keeps no GitHub accounts.
    pub(super) older: bool,
}

impl Drawing<'_> {
    /// Whether the page can ask the hub to change something now: there's a hub, and it keeps GitHub accounts.
    pub(super) fn can_ask(&self, cx: &App) -> bool {
        self.github.is_some() && self.kept.read(cx).can_ask()
    }

    /// Whether `account` is the default, as the hub's settings say.
    pub(super) fn is_default(&self, account: &GithubAccount) -> bool {
        self.github.is_some_and(|github| github.account.as_ref() == Some(&account.id))
    }

    /// What the hub sends every worker for the default, as its settings say, while they say it of the default they
    /// name, as [`GithubSettings::sent_for_default`] says.
    pub(super) fn sent(&self) -> Option<&SentCredential> {
        self.github?.sent_for_default()
    }

    /// How `account`'s login stands: the default's as the hub says what workers are sent for it, and, while it
    /// doesn't say, as from a hub from before it did, and any other's, as its record says at the page's moment.
    pub(super) fn standing(&self, account: &GithubAccount, cx: &App) -> Standing {
        match self.sent().filter(|_| self.is_default(account)) {
            Some(sent) => Standing::sent(sent, account.held_login.as_ref(), cx),
            None => Standing::of(account.held_login.as_ref(), self.now_ms, cx),
        }
    }
}

impl RenderOnce for GitPage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let kept = window.use_keyed_state("settings-git", cx, |_, _| Kept::default());
        let now_ms = look::now_ms(cx);
        let settings = self.records.hub_settings();
        let github = settings.and_then(|settings| settings.github.as_ref());
        let changes_at_ms = changes_at_ms(self.records.github_accounts(), now_ms);
        let focus = self.within.focus.clone();
        kept.update(cx, |kept, cx| kept.drawn(self.asking, focus, &self.records, changes_at_ms, cx));
        let drawing = Drawing {
            kept: &kept,
            records: &self.records,
            scroll: &self.within.scroll,
            now_ms,
            github,
            older: settings.is_some() && github.is_none(),
        };
        v_flex()
            .w_full()
            .max_w(COLUMN_WIDTH)
            .gap(look::Space::XXLarge)
            .pb(look::Space::Large)
            .child(added::list(&drawing, cx))
            .children(found::list(&drawing, cx))
    }
}

/// When what the rows of `accounts` say of a time next changes, after `now_ms`: as a login ends, so a token's says it
/// has, and a sign-in's that it's being renewed, until the hub says it has been.
pub(super) fn changes_at_ms(accounts: &[GithubAccount], now_ms: i64) -> Option<i64> {
    let ends = accounts.iter().filter_map(|account| account.held_login.and_then(|held| held.ends_at_ms));
    ends.filter(|ends_at_ms| *ends_at_ms > now_ms).min()
}
