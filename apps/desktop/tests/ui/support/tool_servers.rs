//! A stand-in for an MCP server and its authorization server, with the test doing what the browser would: opening the
//! page the app opens, where the stand-in has the person approve or refuse, and going where that sends them back.

#![cfg(test)]

use std::time::Duration;

use desktop::browsing::{Fetched, fetch};
use hub::{ToolServerPage, ToolServerRefreshing, ToolServerScript, ToolServerStandIn};
use process::testing::WholeEnvironment;
use protocol::client::tool_sign_ins::{ToolSignInBegun, ToolSignInReturned, ToolSignInTo, redirect};
use protocol::model::ToolServerId;
use url::Url;

use super::{Stack, hub_environment};

/// The server and its authorization server, which serve until the test ends.
pub(crate) struct Serving {
    pub(crate) stand_in: ToolServerStandIn,
    _serving: tokio::runtime::Runtime,
}

impl Serving {
    pub(crate) fn with(script: ToolServerScript) -> Self {
        let serving = tokio::runtime::Runtime::new().unwrap();
        let stand_in = serving.block_on(ToolServerStandIn::with(script));
        Self { stand_in, _serving: serving }
    }

    /// One whose tokens end sooner than a hub [`renewing_often`] renews one, so each check does.
    pub(crate) fn with_short_tokens() -> Self {
        Self::with(ToolServerScript { lasts: Some(60), ..ToolServerScript::default() })
    }

    pub(crate) fn url(&self) -> String {
        self.stand_in.server().as_str().to_string()
    }

    /// Where the person is sent back to from `page`, once they've done on it what the script says.
    pub(crate) fn sent_back_to(&self, page: &str) -> SentBack {
        SentBack { to: redirect_of(page), query: self.stand_in.visit(&ToolServerPage::new(page)) }
    }

    /// Signs in to it as `Linear`, from another client.
    pub(crate) fn signed_in_elsewhere(&self, stack: &Stack) -> ToolServerId {
        let (client, id) = (stack.client(), client_core::new_tool_sign_in_id());
        let to = ToolSignInTo::New { id: "t".into(), name: "Linear".into(), url: self.url() };
        let begun = client.tool_sign_in_start(&id, to, redirect(17867)).unwrap();
        let ToolSignInBegun::Page { page } = begun else { panic!("{begun:?}") };
        let returned = self.stand_in.visit(&ToolServerPage::new(page.as_str()));
        client.tool_sign_in_finish(&id, ToolSignInReturned::new(returned)).unwrap().id
    }

    /// Refuses to renew a sign-in from now on, so the hub finds it's gone.
    pub(crate) fn refuses_to_renew(&self) {
        self.stand_in.change(|script| script.refreshing = ToolServerRefreshing::Refused("invalid_grant"));
    }
}

/// The environment of a hub that looks very often for the sign-ins to renew.
pub(crate) fn renewing_often() -> WholeEnvironment {
    let (timers, short) =
        hub::Timers { check_tool_sign_ins_every: Duration::from_millis(50), ..hub::Timers::FOR_TESTS }.env();
    let mut environment = hub_environment();
    environment.set(timers, short);
    environment
}

/// Where a page sends the person back to.
pub(crate) struct SentBack {
    pub(crate) to: String,
    pub(crate) query: String,
}

impl SentBack {
    pub(crate) fn follow(&self) -> Fetched {
        fetch(&format!("{}?{}", self.to, self.query))
    }
}

/// Where `page` sends the person back to.
pub(crate) fn redirect_of(page: &str) -> String {
    let page = Url::parse(page).unwrap();
    page.query_pairs().find(|(name, _)| name == "redirect_uri").unwrap().1.into_owned()
}
