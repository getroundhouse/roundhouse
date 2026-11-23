//! Whose account an agent runs under, the login the hub holds for it, and how much of its limits is left.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{AccountId, AgentKind, HeardLimits, Rev};

/// The account an agent's CLI is logged in to on its worker's machine, as the worker reads it. Every part is `None`
/// when the agent doesn't say, such as a plan for a login with an API key. Who it is, from `model_provider` on, is
/// what an [`Account`]'s record says of it too, so a part added here is added there.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccount {
    /// Which account it is. Agents under the same account share its limits, and an agent under another, even with
    /// the same model provider, has its own. `None` when nobody is logged in, or the agent can't tell one login from
    /// another.
    #[serde(default)]
    pub id: Option<AccountId>,
    /// Whether anyone is logged in.
    #[serde(default)]
    pub logged_in: bool,
    /// Who serves the model, for a person to read, such as `Anthropic API`. Not a provider in this project's sense,
    /// which is where a hub runs workers.
    #[serde(default)]
    pub model_provider: Option<String>,
    /// The subscription, for a person to read, such as `Max`.
    #[serde(default)]
    pub plan: Option<String>,
    /// How it's logged in, for a person to read, such as `Claude account`.
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

impl AgentAccount {
    /// Which account it is, when someone is logged in to one that can be told from another. Only such an account has
    /// a record, and limits kept for it.
    pub fn logged_in_as(&self) -> Option<&AccountId> {
        self.id.as_ref().filter(|_| self.logged_in)
    }
}

/// An account agents run under, as a record the hub keeps, so every client shows the same of it and it's there once
/// the agents have gone. Who it is comes from the worker, read each time an agent under it launches, or for a login
/// the hub holds, from the hub's record of that login alone, since a token doesn't say whose it is. What's left of its
/// limits is what any agent under it last heard, in whichever workspace, since a limit is the account's rather than a
/// session's. An adapter's account has a record only for a login that can be told from another, and the hub's has one
/// for each login it was given, which stays once the login is disconnected. Who it is, from `model_provider` to
/// `email`, is what [`AgentAccount`] says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    /// Who serves the model, for a person to read, as [`AgentAccount`] says.
    #[serde(default)]
    pub model_provider: Option<String>,
    /// The subscription, for a person to read.
    #[serde(default)]
    pub plan: Option<String>,
    /// How it's logged in, for a person to read.
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    /// What the person called the account as they connected a login to it, since a token doesn't say whose it is. It
    /// stays once the login is disconnected, since the sessions that ran under it still name the account.
    #[serde(default)]
    pub name: Option<String>,
    /// The kind of agent the hub was given the account's login for, which says the hub chose the account's ID for a
    /// login it holds, or held, rather than an adapter from the login on a worker's machine. It stays once the login
    /// is disconnected, as the name does. A client renewing the login reads which kind's command makes a token here.
    #[serde(default)]
    pub login_agent: Option<AgentKind>,
    /// Shortest window first. Empty until an agent under the account has heard of one, and for an account with none.
    /// None says when it was heard, since hearing the same again isn't a change to the record.
    #[serde(default)]
    pub limits: Vec<UsageLimit>,
    /// The login the hub holds for the account and hands to workers whose machine has none of its own, if it holds one.
    /// `None` for an account that agents run under through the login on their worker's machine, and for one whose
    /// login was disconnected.
    #[serde(default)]
    pub held_login: Option<HeldLogin>,
    /// When the person last sent something in a session that names the account, by the hub's clock, so a client can
    /// say when the account was last used. What they sent before the session's first launch counts once that launch
    /// names it. `None` until they have, and from a hub from before it kept it.
    #[serde(default)]
    pub used_at_ms: Option<i64>,
    /// How much of the account's week the person keeps for themselves, as [`Reserve`] says. `None` keeps none back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserve: Option<Reserve>,
    #[serde(default)]
    pub rev: Rev,
}

/// How much of an account's week, its [`LimitWindow::SevenDays`] limit, the person keeps for themselves, from 1 to 100
/// percent. Automatic puts no new chat on the account once no more than this is left. It's the hub's rule of where it
/// puts chats, not a reservation the provider knows of, so chats already on the account, and use outside Roundhouse,
/// can still spend it. 100 keeps the account out of Automatic.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "u8", into = "u8")]
pub struct Reserve(u8);

impl Reserve {
    /// All of the week, which keeps the account out of Automatic.
    pub const ALL: Reserve = Reserve(100);

    /// `percent` of the week, or `None` for none, or more than all of it.
    pub const fn new(percent: u8) -> Option<Reserve> {
        if percent >= 1 && percent <= 100 { Some(Reserve(percent)) } else { None }
    }

    /// From 1 to 100.
    pub fn percent(self) -> u8 {
        self.0
    }

    /// Whether a week with `left_percent` left is down to it, so what's spent from here is what's reserved. The one
    /// rule for it, which the hub places chats by and every client warns by, so they agree.
    pub fn reached_at(self, left_percent: u8) -> bool {
        left_percent <= self.0
    }
}

impl TryFrom<u8> for Reserve {
    type Error = ReserveOutOfRange;

    fn try_from(percent: u8) -> Result<Self, Self::Error> {
        Reserve::new(percent).ok_or(ReserveOutOfRange(percent))
    }
}

impl From<Reserve> for u8 {
    fn from(reserve: Reserve) -> u8 {
        reserve.0
    }
}

/// A reserve of none of a week, or more than all of it, which isn't one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a reserve is from 1 to 100 percent of the week, not {0}")]
pub struct ReserveOutOfRange(pub u8);

/// What an [`Account`]'s record says of the login the hub holds for it. It never carries the token, which only the
/// hub and the workers it hands it to have.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldLogin {
    /// When the hub was given it, by the hub's clock.
    pub connected_at_ms: i64,
    /// When it stops working at the latest, by the hub's clock, if the hub knows: as long after it was connected as
    /// [`LoginDescription::lasts_ms`] says a token of its kind lasts. One made before it was connected ends sooner.
    ///
    /// [`LoginDescription::lasts_ms`]: super::LoginDescription::lasts_ms
    #[serde(default)]
    pub ends_at_ms: Option<i64>,
}

impl HeldLogin {
    /// Whether it still works at `now_ms`, as far as the record says: it hasn't ended, or nobody knows when it does.
    /// When it ends is by the hub's clock, so a client asking by its own is off by however far the two clocks are apart.
    pub fn works_at(&self, now_ms: i64) -> bool {
        self.ends_at_ms.is_none_or(|ends_at_ms| ends_at_ms > now_ms)
    }
}

impl Account {
    /// What any agent under it last heard of its week, as [`HeardLimits::week`] says.
    pub fn week(&self, now_ms: i64) -> Option<&UsageLimit> {
        HeardLimits(&self.limits).week(now_ms)
    }

    /// Its limits that are used up at `now_ms`, as [`HeardLimits::used_up`] says.
    pub fn used_up(&self, now_ms: i64) -> impl Iterator<Item = &UsageLimit> {
        HeardLimits(&self.limits).used_up(now_ms)
    }

    /// Whether what's left of its week at `now_ms` is down to what it reserves, as [`HeardLimits::in_reserve`] says.
    pub fn in_reserve(&self, now_ms: i64) -> bool {
        HeardLimits(&self.limits).in_reserve(self.reserve, now_ms)
    }

    /// Whether it's a login added for `kind` that still works at `now_ms`, as [`HeldLogin::works_at`] says, so it can
    /// be chosen as that kind's default, or a chat of that kind moved to it. The hub also needs the login's token,
    /// which a client can't see, so it can still refuse one.
    pub fn works_for(&self, kind: AgentKind, now_ms: i64) -> bool {
        self.login_agent == Some(kind) && self.held_login.is_some_and(|held| held.works_at(now_ms))
    }
}

/// Where the login a chat's agent runs under comes from, as [`Session::account_source`] says.
///
/// [`Session::account_source`]: super::Session::account_source
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountSource {
    /// The agent's CLI's own credentials, from the environment of the machine it runs on, such as a login the person
    /// made there. The hub hands it none, and learns whose it is only from the worker, after each launch.
    Ambient,
    /// The login the hub holds for an account a person added, which [`Session::account`] names.
    ///
    /// [`Session::account`]: super::Session::account
    Hub,
    /// One from a newer peer.
    #[serde(other)]
    Unknown,
}

/// A login the hub holds for an account, as it hands it to a worker for an agent or a one-off call to run under, so the
/// worker's machine needs no login of its own. The hub gives one only to a worker, which keeps it only while the agent
/// or the call it came with runs. No provider's adapter is given one, though one that carries a worker's stream itself
/// passes it on unread.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentLogin {
    /// A token the agent's own CLI made to last, such as `claude setup-token`'s, which the agent's adapter gives the CLI
    /// in its own way.
    pub token: LoginToken,
}

crate::tokens!(
    /// The token of a login the hub holds for an agent's account, which is all it takes to use the account, and which
    /// the hub hands to the agent's CLI.
    LoginToken,
);

/// What a session's agent runs under: the account, and what the hub has heard of that account's limits, from any
/// agent under it. Kept for clients from before accounts were records, which ask for it. A client now reads
/// [`Session::account`](super::Session::account) and that account's record.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionAccount {
    pub account: AgentAccount,
    /// Shortest window first. Empty until an agent under the account has heard of one, and for an account with none.
    #[serde(default)]
    pub limits: Vec<UsageLimit>,
}

/// How much of one limit on an account's use is used up.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UsageLimit {
    pub window: LimitWindow,
    /// The model whose use alone a [`LimitWindow::ModelSevenDays`] limit counts. `None` for any other window. Each
    /// model's week is a limit of its own, so one without a model can't be told from another's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelLabel>,
    /// From 0 to 100.
    pub used_percent: u8,
    /// When the window ends and the limit starts again.
    #[serde(default)]
    pub resets_at_ms: Option<i64>,
    /// When an agent heard this, by the hub's clock, which the hub adds as it keeps it. `None` from the agent, which
    /// says it as it hears it, and in an [`Account`]'s record.
    #[serde(default)]
    pub heard_at_ms: Option<i64>,
}

impl UsageLimit {
    /// How much is left, from 0 to 100.
    pub fn left_percent(&self) -> u8 {
        100_u8.saturating_sub(self.used_percent)
    }
}

/// How long a limit counts use over, and of what, shortest first, with one model's own week after the week of all of
/// them.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LimitWindow {
    FiveHours,
    SevenDays,
    /// The week's use of one model alone, the limit's [`UsageLimit::model`]. An account can run out of it while the
    /// week of all of them has some left, and then that model stops, or spends usage credits where the account has
    /// them, while the others go on.
    ModelSevenDays,
    #[serde(other)]
    Unknown,
}

/// A model as its provider labels it for people, such as `Fable`, which a limit on one model's use names. It's the
/// provider's, shown as it comes, so a new model's limit needs no new build to be named. It isn't a
/// [`ModelName`](super::ModelName), which is what an agent is asked to run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ModelLabel(String);

impl ModelLabel {
    pub fn new(label: impl Into<String>) -> Self {
        Self(label.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whats_left_of_a_limit_is_the_rest_of_it() {
        let limit = |used_percent| UsageLimit {
            window: LimitWindow::SevenDays,
            model: None,
            used_percent,
            resets_at_ms: None,
            heard_at_ms: None,
        };

        assert_eq!(limit(4).left_percent(), 96);
        assert_eq!(limit(100).left_percent(), 0);
        assert_eq!(limit(140).left_percent(), 0);
    }

    #[test]
    fn a_reserve_is_from_one_to_all_of_the_week_and_an_account_is_in_it_once_its_week_is_down_to_it() {
        let week = |used_percent, resets_at_ms| UsageLimit {
            window: LimitWindow::SevenDays,
            model: None,
            used_percent,
            resets_at_ms,
            heard_at_ms: None,
        };
        let reserving = |reserve, limits| Account { limits, reserve, ..crate::fixtures::account("a") };

        assert_eq!([0, 1, 100, 101].map(Reserve::new), [None, Reserve::new(1), Some(Reserve::ALL), None]);
        assert!(serde_json::from_str::<Reserve>("0").is_err());
        assert!(serde_json::from_str::<Reserve>("101").is_err());
        assert_eq!(serde_json::to_string(&Reserve::new(25)).unwrap(), "25");
        let in_reserve = |reserve, limits| reserving(Reserve::new(reserve), limits).in_reserve(1_000);
        assert!(!in_reserve(25, vec![week(74, Some(2_000))]), "26% left of a quarter reserved");
        assert!(in_reserve(25, vec![week(75, Some(2_000))]), "down to it");
        assert!(!in_reserve(25, vec![week(90, Some(1_000))]), "a week that has started again since");
        assert!(!in_reserve(25, Vec::new()), "a week nobody has heard");
        assert!(!reserving(None, vec![week(100, None)]).in_reserve(1_000), "none reserved");
    }

    #[test]
    fn an_account_that_says_nothing_still_reads_and_a_newer_window_is_unknown() {
        assert_eq!(serde_json::from_str::<AgentAccount>("{}").unwrap(), AgentAccount::default());
        assert_eq!(serde_json::from_str::<SessionAccount>(r#"{"account":{}}"#).unwrap(), SessionAccount::default());
        let newer: UsageLimit = serde_json::from_str(r#"{"window":"thirty_days","used_percent":3}"#).unwrap();

        assert_eq!(newer.window, LimitWindow::Unknown);
    }

    #[test]
    fn a_models_week_names_its_model_on_the_wire_and_another_window_says_nothing_of_one() {
        let week = UsageLimit {
            window: LimitWindow::SevenDays,
            model: None,
            used_percent: 4,
            resets_at_ms: None,
            heard_at_ms: None,
        };
        let fables = UsageLimit { window: LimitWindow::ModelSevenDays, model: Some(ModelLabel::new("Fable")), ..week };

        assert_eq!(
            serde_json::to_value(&fables).unwrap(),
            serde_json::json!({"window": "model_seven_days", "model": "Fable", "used_percent": 4, "resets_at_ms": null,
                "heard_at_ms": null})
        );
        assert!(serde_json::to_value(&week).unwrap().get("model").is_none());
    }

    #[test]
    fn an_account_record_that_says_only_who_it_is_still_reads() {
        let account: Account = serde_json::from_str(r#"{"id":"claude-1"}"#).unwrap();

        assert_eq!(account.id, AccountId::from("claude-1"));
        assert_eq!((account.email, account.limits, account.rev), (None, Vec::new(), Rev::default()));
        assert_eq!((account.name, account.login_agent, account.held_login), (None, None, None));
    }

    #[test]
    fn windows_sort_shortest_first() {
        let mut windows = [LimitWindow::SevenDays, LimitWindow::FiveHours];
        windows.sort();

        assert_eq!(windows, [LimitWindow::FiveHours, LimitWindow::SevenDays]);
    }
}
