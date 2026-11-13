//! What Accounts says in words: what each list says of itself, what each account is called, how its login stands,
//! when it was last used, how long a token lasts, what disconnecting asks, and what the hub's refusals mean.

use client_core::{Agents, ClientError};
use gpui_kit::App;
use protocol::client::ErrorCode;
use protocol::model::{Account, HeldLogin};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const YEAR_MS: i64 = 365 * DAY_MS;
/// The longest before a login ends that its row warns of it: a month, so there's time to make a new token.
const WARNS_AT_MOST_MS: i64 = 30 * DAY_MS;

/// What a search finds the page by, besides its name.
pub(super) const ABOUT: &str = "Logins agents run under: those found where they ran, and those given to the hub with \
                                a token, which can be connected, renewed and disconnected.";

/// What a hub that can't keep a login is said to be.
pub(super) const OLDER_HUB: &str =
    "This hub is from before hubs kept logins, so it can't connect an account until it's updated.";

/// What Added says while the person has given the hub no login.
pub(super) const NONE_ADDED: &str = "None yet. Connect an account to run agents on a machine with no login of its own.";

/// What Added says of itself under its heading: what the hub does with these, as it decides, which nothing here
/// chooses. This machine is named as the app names it everywhere.
pub(super) fn added_says() -> String {
    let machine = crate::names::MACHINE;
    format!(
        "Logins you've given the hub. It hands the newest that hasn't ended to agents on any machine without a login \
         of its own, this {machine} included, and a machine with one keeps using its own."
    )
}

/// What disconnecting the account called `name` asks first.
pub(super) fn disconnect_asks(name: &str) -> String {
    format!("Disconnect {name}?")
}

/// What the question before disconnecting says under it.
pub(super) const DISCONNECT_SAYS: &str = "The hub forgets its token, so connecting it again takes pasting one again. \
                                          Agents running under it go on until they stop. The token isn't revoked, \
                                          so it still works wherever else it's kept.";

/// What's said under an account's row when it was to be disconnected with no hub to ask.
pub(super) const NO_HUB_TO_DISCONNECT: &str = "Couldn't disconnect it: there's no hub to ask.";

/// What an account is called on its row: who it is, as the app says it everywhere, such as the name the person gave it
/// for one whose login they added, and otherwise what else its record says.
pub(super) fn called(account: &Account) -> String {
    crate::names::account(account).or_else(|| account.login.clone()).unwrap_or_else(|| account.id.to_string())
}

/// How a login the person added stands, as its row says, and how loudly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Standing {
    /// The hub holds it, and it ends after its row starts to warn of it, as this says, or the hub doesn't know when.
    Held(Option<String>),
    /// The hub holds it, and it ends soon, as [`warns_from_ms`] says, which is said in amber.
    EndingSoon(String),
    /// The hub holds it, and it has ended, which is said in red. No agent runs under it until it's renewed.
    Ended(String),
    /// The hub holds none for it, since it was disconnected.
    Disconnected,
}

/// When the row of `held` starts to warn that it's ending, if the hub knows when it ends: a twelfth of how long it lasts
/// before, and at most a month, so one that lasts a year warns a month before and one that lasts a week isn't a
/// warning from the moment it's connected.
pub(super) fn warns_from_ms(held: &HeldLogin) -> Option<i64> {
    let ends_at_ms = held.ends_at_ms?;
    let lasts_ms = ends_at_ms.saturating_sub(held.connected_at_ms).max(0);
    Some(ends_at_ms - (lasts_ms / 12).min(WARNS_AT_MOST_MS))
}

/// How `held`, the login the hub holds for an account, stands at `now_ms`, with when it ends in the local time zone.
pub(super) fn standing(held: Option<&HeldLogin>, now_ms: i64, cx: &App) -> Standing {
    let Some(held) = held else { return Standing::Disconnected };
    let (Some(ends_at_ms), Some(warns_from_ms)) = (held.ends_at_ms, warns_from_ms(held)) else {
        return Standing::Held(None);
    };
    let Some(on) = look::date(ends_at_ms, cx) else { return Standing::Held(None) };
    if ends_at_ms <= now_ms {
        return Standing::Ended(format!("Ended on {on}"));
    }
    if now_ms < warns_from_ms {
        return Standing::Held(Some(format!("Ends {on}")));
    }
    // By the calendar, so it says the same day as the date beside it.
    Standing::EndingSoon(match look::days_apart(now_ms, ends_at_ms, cx) {
        Some(0) => "Ends today".to_string(),
        Some(1) => "Ends tomorrow".to_string(),
        Some(days) => format!("Ends in {days} days, on {on}"),
        None => format!("Ends {on}"),
    })
}

/// Which day the person last sent something in a chat under the account, such as "Last used yesterday". `None`
/// until they have, and from a hub from before it kept that, which can't be told apart, so nothing is said of either.
pub(super) fn last_used(account: &Account, now_ms: i64, cx: &App) -> Option<String> {
    let day = crate::ago::day_ago(account.used_at_ms?, now_ms, cx)?;
    Some(format!("Last used {day}"))
}

/// How long a token lasts, in whole years or days, such as "a year". `None` for one that lasts no whole number of days.
pub(super) fn lasting(lasts_ms: i64) -> Option<String> {
    let (count, unit) = match lasts_ms {
        ..=0 => return None,
        _ if lasts_ms % YEAR_MS == 0 => (lasts_ms / YEAR_MS, "year"),
        _ if lasts_ms % DAY_MS == 0 => (lasts_ms / DAY_MS, "day"),
        _ => return None,
    };
    Some(if count == 1 { format!("a {unit}") } else { format!("{count} {unit}s") })
}

/// The name of the one kind of agent the hub describes, as `agents`, to say whose login is meant, or `None` when it
/// describes several, or none. Found has every kind's login, whether or not the hub can be given one of its kind.
fn the_one_agent(agents: &Agents) -> Option<&str> {
    let mut named = agents.named();
    let one = named.next()?;
    named.next().is_none().then_some(one.name.as_str())
}

/// What Found says of itself under its heading.
pub(super) fn found_says(agents: &Agents) -> String {
    let whose = the_one_agent(agents).unwrap_or("Agents' CLIs");
    format!("The logins {whose} already had where agents ran. Roundhouse reads who they are, and never copies them.")
}

/// What Found says while it lists none. The hub learns of one only as an agent launches under it, so there's no
/// telling whether a machine has one before then.
pub(super) fn none_found(agents: &Agents) -> String {
    match the_one_agent(agents) {
        Some(name) => format!("None found yet. {name}'s own login shows here once an agent has run under it."),
        None => "None found yet. An agent's own login shows here once an agent has run under it.".to_string(),
    }
}

/// What the hub's refusal to connect a login means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Refused {
    /// The hub is from before hubs kept logins, so it can't connect any.
    OlderHub,
    /// This, which the panel says.
    Said(String),
}

/// What to say of the hub's refusal to connect a login, as a new account or, when `renewing`, in place of an account's.
/// Its message is said where it's meant for a person, as one for a name or a token it can't take is.
pub(super) fn refused(e: &ClientError, renewing: bool) -> Refused {
    let ClientError::Api(api) = e else { return Refused::Said(format!("Couldn't connect it: {e}")) };
    Refused::Said(match api.code {
        ErrorCode::Unsupported => return Refused::OlderHub,
        ErrorCode::Conflict if renewing => {
            "Another account holds this token already. Make a new one for this account.".to_string()
        }
        ErrorCode::Conflict => "This token is connected already, under another name.".to_string(),
        ErrorCode::NotFound => "The hub no longer has this account.".to_string(),
        ErrorCode::InvalidRequest
        | ErrorCode::VersionMismatch
        | ErrorCode::NeedsForce
        | ErrorCode::NameTaken
        | ErrorCode::Worker
        | ErrorCode::Unavailable
        | ErrorCode::Internal
        | ErrorCode::Unknown => format!("Couldn't connect it: {}", api.message),
    })
}

/// What to say when the hub couldn't forget an account's login.
pub(super) fn disconnect_failed(e: &ClientError) -> String {
    if e.is_not_found() {
        return "The hub no longer has this account.".to_string();
    }
    format!("Couldn't disconnect it: {e}")
}
