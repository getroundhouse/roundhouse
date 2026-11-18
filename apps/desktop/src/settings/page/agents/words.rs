//! What Agents says in words: what each list says of itself, how an account's login was made and how it stands, what
//! else its row says, when its card says it was last used and its login ends, how long a token lasts, what
//! disconnecting asks, and what the hub's refusals mean for a login, with why it refused otherwise as client-core's
//! `refusals` says it.

use client_core::{Agents, ClientError, refusals};
use gpui_kit::{App, SharedString};
use protocol::client::ErrorCode;
use protocol::model::{Account, HeldLogin, Reserve};

use super::super::account_row::Tone;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const YEAR_MS: i64 = 365 * DAY_MS;
/// The longest before a login ends that its row warns of it: a month, so there's time to make a new token.
const WARNS_AT_MOST_MS: i64 = 30 * DAY_MS;

/// What a search finds the page by, besides its name.
pub(super) const ABOUT: &str = "The accounts agents run under: the logins found where they ran, and those given to the \
                                hub with a token, which can be connected, renewed and disconnected.";

/// What a hub that can't keep a login is said to be.
pub(super) const OLDER_HUB: &str =
    "This hub is from before hubs kept logins, so it can't connect an account until it's updated.";

/// What Added says while the person has given the hub no login.
pub(super) const NONE_ADDED: &str = "None yet. Connect an account to add its login to this hub.";

/// What Added says of itself under its heading: the hub applies its selected defaults before machine credentials.
pub(super) fn added_says(defaults: bool) -> String {
    if defaults {
        "Logins kept by this hub. The default for each agent takes priority over machine credentials."
    } else {
        "Logins kept by this hub. Update the hub to choose defaults that take priority over machine credentials."
    }
    .to_string()
}

/// What disconnecting the account called `name` asks first.
pub(super) fn disconnect_asks(name: &str) -> String {
    format!("Disconnect {name}?")
}

/// What the question before disconnecting says under it.
pub(super) const DISCONNECT_SAYS: &str = "The hub forgets its token, so connecting it again takes pasting one again. \
                                          Agents running under it go on until they stop. The token isn't revoked, \
                                          so it still works wherever else it's kept. Chats on it can't start until \
                                          you reconnect it or move them, and if it's a default account, new chats \
                                          can't either until you reconnect it or choose another default.";

/// What an account's row and its card say, faintly, while no agent under it has heard a limit.
pub(super) const NO_LIMITS_YET: &str = "No limits heard of yet";

/// What's said under an account's row when it was to be disconnected with no hub to ask.
pub(super) const NO_HUB_TO_DISCONNECT: &str = "Couldn't disconnect it: there's no hub to ask.";

/// How a login the person added stands, as its row says, and how loudly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Standing {
    /// The hub holds it, and it ends after its row starts to warn of it, so only its card says when, as `Ends Oct 12,
    /// 2027`, or the hub doesn't know when, so nothing says it.
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

/// How an account's login was made, as its row says: the kind of agent it's for, for one the person added, then its
/// plan and how it's logged in, where its record says, such as `Max · Claude account` for one found where agents ran.
pub(super) fn how(account: &Account, agents: &Agents) -> Option<String> {
    let kind = account.login_agent.and_then(|kind| agents.of(kind)).map(|agent| agent.name.clone());
    let parts: Vec<String> = [kind, account.plan.clone(), account.login.clone()].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// What else an account's row says, after how its login was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Says {
    /// How the login the person added stands, as loudly as the tone says: that it ends soon or has ended, or that it
    /// was disconnected.
    Standing(String, Tone),
    /// How much is left of each limit there's something true to say of, by its window, such as `5h`, as a percentage,
    /// with how much of it the account keeps back, for its week.
    Limits(Vec<(SharedString, u8, Option<Reserve>)>),
    /// That no agent under it has heard a limit yet, faintly.
    NoLimitsYet,
    /// Nothing, since every limit heard has started again.
    Nothing,
}

/// What else the row of `account`, whose login stands as `standing` says, if the person added it, says at `now_ms`: how
/// its login stands, once that needs saying, since an agent can't run under it once it has ended or been disconnected,
/// and otherwise what's left of its limits, as the chat's card of them would show them.
pub(super) fn says(account: &Account, standing: Option<&Standing>, now_ms: i64) -> Says {
    match standing {
        Some(Standing::EndingSoon(said)) => return Says::Standing(said.clone(), Tone::Warning),
        Some(Standing::Ended(said)) => return Says::Standing(said.clone(), Tone::Problem),
        Some(Standing::Disconnected) => return Says::Standing("Disconnected".to_string(), Tone::Plain),
        Some(Standing::Held(_)) | None => {}
    }
    if account.limits.is_empty() {
        return Says::NoLimitsYet;
    }
    let shown = crate::limits::shown_by_window(&account.limits, now_ms);
    if shown.is_empty() {
        return Says::Nothing;
    }
    let each = shown.into_iter().map(|(window, limit)| {
        let reserve = crate::limits::reserve_for(account, &limit);
        (window, limit.left_percent(), reserve)
    });
    Says::Limits(each.collect())
}

/// What's said for `limits`, each by its window and how much of it is left, as `Says::Limits` holds them, to assistive
/// apps, which can't see the bars: `5h limit, 71% left, 7d limit, 96% left`.
pub(super) fn limits_left(limits: &[(SharedString, u8, Option<Reserve>)]) -> String {
    let each = limits.iter().map(|(window, left, reserve)| match reserve {
        Some(reserve) => format!("{window} limit, {left}% left, {}% reserved", reserve.percent()),
        None => format!("{window} limit, {left}% left"),
    });
    each.collect::<Vec<_>>().join(", ")
}

/// Which day the person last sent something in a chat under the account, as its card says, such as "Last used
/// yesterday". `None` until they have, and from a hub from before it kept that, which can't be told apart, so nothing
/// is said of either.
pub(super) fn last_used(account: &Account, now_ms: i64, cx: &App) -> Option<String> {
    let day = crate::ago::day_ago(account.used_at_ms?, now_ms, cx)?;
    Some(format!("Last used {day}"))
}

/// When the login the person added ends, or ended, as its card says, whether or not its row warns of it yet, from how
/// it stands. `None` for one the hub holds none of, or doesn't know the end of.
pub(super) fn ends(standing: &Standing) -> Option<&str> {
    match standing {
        Standing::Held(ends) => ends.as_deref(),
        Standing::EndingSoon(ends) | Standing::Ended(ends) => Some(ends),
        Standing::Disconnected => None,
    }
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
    let conflict = matches!(e, ClientError::Api(api) if api.code == ErrorCode::Conflict);
    Refused::Said(match e {
        e if e.is_unsupported() => return Refused::OlderHub,
        _ if conflict && renewing => {
            "Another account holds this token already. Make a new one for this account.".to_string()
        }
        _ if conflict => "This token is connected already, under another name.".to_string(),
        e if e.is_not_found() => refusals::gone("account"),
        e => refusals::couldnt("connect it", e),
    })
}

/// What to say when the hub couldn't forget an account's login.
pub(super) fn disconnect_failed(e: &ClientError) -> String {
    if e.is_not_found() {
        return refusals::gone("account");
    }
    refusals::couldnt("disconnect it", e)
}
