//! What the page says in words: what an account is called, how its login stands, how long a token lasts, what the
//! hub's refusals mean, and what Found says of itself.

use client_core::{Agents, ClientError};
use gpui_kit::TestAppContext;
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::account;
use protocol::model::{Account, AgentDescription, AgentKind, HeldLogin};

use super::super::older_hub;
use super::super::words::{self, Refused, Standing};
use super::made_up::{DAY_MS, MINUTE_MS, NOW_MS, added, claude, holding, setup_token};

/// What `account` is called, as a client that holds it reads it.
fn called(account: &Account) -> String {
    crate::names::called(&holding([account]), account)
}

#[test]
fn an_added_account_is_called_by_its_name_and_a_found_one_by_its_email() {
    let both = |account: Account| Account { email: Some("ada@example.com".to_string()), ..account };

    assert_eq!(called(&both(added("a", "Work"))), "Work");
    assert_eq!(called(&both(Account { name: Some("Work".to_string()), ..account("a") })), "ada@example.com");
    assert_eq!(called(&Account { login: Some("API key".to_string()), ..account("a") }), "API key");
    assert_eq!(called(&account("claude-1")), "claude-1", "with nothing else to go by");
}

#[test]
fn a_hub_is_from_before_hubs_kept_logins_when_it_describes_no_kind_with_one_or_says_so() {
    assert!(!older_hub(&Agents::default(), false), "nothing described, as before the hub has said");
    assert!(older_hub(&Agents::new(vec![claude(None)]), false), "Claude, with nothing that makes a token");
    assert!(!older_hub(&Agents::new(vec![claude(Some(setup_token()))]), false));
    assert!(older_hub(&Agents::new(vec![claude(Some(setup_token()))]), true), "it refused to connect one");
}

/// How a login that lasts a year and ends at `ends_at_ms` stands at [`NOW_MS`], with times in UTC.
fn standing(cx: &mut TestAppContext, ends_at_ms: Option<i64>) -> Standing {
    let connected_at_ms = ends_at_ms.unwrap_or(NOW_MS) - 365 * DAY_MS;
    standing_of(cx, HeldLogin { connected_at_ms, ends_at_ms })
}

/// How `held` stands at [`NOW_MS`], with times in UTC.
fn standing_of(cx: &mut TestAppContext, held: HeldLogin) -> Standing {
    cx.update(|cx| {
        look::hold_still(NOW_MS, cx);
        words::standing(Some(&held), NOW_MS, cx)
    })
}

#[gpui_kit::test]
fn a_login_warns_once_its_within_a_month_of_ending(cx: &mut TestAppContext) {
    assert_eq!(
        standing(cx, Some(NOW_MS + 345 * DAY_MS)),
        Standing::Held(Some("Ends Oct 12, 2027".to_string())),
        "which only its card says"
    );
    assert_eq!(standing(cx, Some(NOW_MS + 30 * DAY_MS + 1)), Standing::Held(Some("Ends Dec 1, 2026".to_string())));
    assert_eq!(
        standing(cx, Some(NOW_MS + 30 * DAY_MS)),
        Standing::EndingSoon("Ends in 30 days, on Dec 1, 2026".into())
    );
    assert_eq!(standing(cx, Some(NOW_MS + 9 * DAY_MS)), Standing::EndingSoon("Ends in 9 days, on Nov 10, 2026".into()));
    assert_eq!(standing(cx, None), Standing::Held(None), "the hub doesn't know when it ends");
}

#[gpui_kit::test]
fn a_login_that_lasts_under_a_year_warns_a_twelfth_of_how_long_it_lasts_before_it_ends(cx: &mut TestAppContext) {
    let lasting = |cx: &mut TestAppContext, lasts_ms: i64, left_ms: i64| {
        let ends_at_ms = NOW_MS + left_ms;
        standing_of(cx, HeldLogin { connected_at_ms: ends_at_ms - lasts_ms, ends_at_ms: Some(ends_at_ms) })
    };

    assert_eq!(lasting(cx, 96 * DAY_MS, 8 * DAY_MS + 1), Standing::Held(Some("Ends Nov 9, 2026".to_string())));
    assert_eq!(lasting(cx, 96 * DAY_MS, 8 * DAY_MS), Standing::EndingSoon("Ends in 8 days, on Nov 9, 2026".into()));
    assert!(matches!(lasting(cx, DAY_MS, 3 * MINUTE_MS * 60), Standing::Held(_)), "a day's warns for two hours");
    assert_eq!(lasting(cx, DAY_MS, 2 * MINUTE_MS * 60), Standing::EndingSoon("Ends today".to_string()));
}

#[gpui_kit::test]
fn a_login_ending_soon_counts_days_by_the_calendar(cx: &mut TestAppContext) {
    let midnight_ms = NOW_MS + 9 * 60 * MINUTE_MS;

    assert_eq!(standing(cx, Some(midnight_ms - MINUTE_MS)), Standing::EndingSoon("Ends today".to_string()));
    assert_eq!(standing(cx, Some(midnight_ms)), Standing::EndingSoon("Ends tomorrow".to_string()));
    assert_eq!(
        standing(cx, Some(midnight_ms + DAY_MS)),
        Standing::EndingSoon("Ends in 2 days, on Nov 3, 2026".to_string()),
        "under two days off, but two days on"
    );
}

#[gpui_kit::test]
fn a_login_has_ended_from_the_moment_it_ends_and_one_the_hub_holds_none_of_is_disconnected(cx: &mut TestAppContext) {
    assert_eq!(standing(cx, Some(NOW_MS)), Standing::Ended("Ended on Nov 1, 2026".to_string()));
    assert_eq!(standing(cx, Some(NOW_MS - 3 * DAY_MS)), Standing::Ended("Ended on Oct 29, 2026".to_string()));
    assert_eq!(cx.update(|cx| words::standing(None, NOW_MS, cx)), Standing::Disconnected);
}

#[gpui_kit::test]
fn which_day_an_account_was_last_used_is_said_and_nothing_before_it_has_been(cx: &mut TestAppContext) {
    let used = |used_at_ms| {
        cx.update(|cx| {
            look::hold_still(NOW_MS, cx);
            words::last_used(&Account { used_at_ms, ..account("a") }, NOW_MS, cx)
        })
    };

    assert_eq!(used(Some(NOW_MS - 125 * MINUTE_MS)).as_deref(), Some("Last used today"));
    assert_eq!(used(Some(NOW_MS - 16 * 60 * MINUTE_MS)).as_deref(), Some("Last used yesterday"));
    assert_eq!(used(Some(NOW_MS - 15 * DAY_MS)).as_deref(), Some("Last used 2 weeks ago"));
    assert_eq!(used(None), None);
}

#[test]
fn what_a_login_lasts_is_said_in_whole_years_or_days() {
    assert_eq!(words::lasting(365 * DAY_MS).as_deref(), Some("a year"));
    assert_eq!(words::lasting(730 * DAY_MS).as_deref(), Some("2 years"));
    assert_eq!(words::lasting(90 * DAY_MS).as_deref(), Some("90 days"));
    assert_eq!(words::lasting(DAY_MS).as_deref(), Some("a day"));
    assert_eq!(words::lasting(DAY_MS + 1), None);
    assert_eq!(words::lasting(0), None);
}

#[test]
fn a_hub_too_old_to_keep_logins_is_told_apart_from_what_it_says_for_a_person() {
    let api = |code, message: &str| ClientError::Api(ApiError::new(code, message));
    let said_renewing = |e: ClientError, renewing| match words::refused(&e, renewing) {
        Refused::Said(said) => said,
        Refused::OlderHub => panic!("{e} isn't from an older hub"),
    };
    let said = |e| said_renewing(e, true);

    assert_eq!(words::refused(&api(ErrorCode::Unsupported, "doesn't know that request"), false), Refused::OlderHub);
    assert_eq!(
        said(api(ErrorCode::InvalidRequest, "the token has a space or a line break inside, which no CLI's has")),
        "Couldn't connect it: the token has a space or a line break inside, which no CLI's has"
    );
    assert!(
        !said(api(ErrorCode::Conflict, "the account claude-login-1 holds that token already")).contains("claude-login")
    );
    assert_eq!(
        said_renewing(api(ErrorCode::Conflict, "that token is connected already, as Work"), false),
        "This token is connected already, under another name.",
        "a new account isn't told to make a new one"
    );
    assert_eq!(said(api(ErrorCode::NotFound, "no such account")), "The hub no longer has this account.");
    assert_eq!(said(ClientError::Disconnected), "Couldn't connect it: the connection to the hub was lost");
}

#[test]
fn why_the_hub_couldnt_disconnect_a_login_is_said_for_a_person() {
    let gone = ClientError::Api(ApiError::new(ErrorCode::NotFound, "no account claude-login-1"));

    assert_eq!(words::disconnect_failed(&gone), "The hub no longer has this account.");
    assert_eq!(
        words::disconnect_failed(&ClientError::Disconnected),
        "Couldn't disconnect it: the connection to the hub was lost"
    );
}

#[test]
fn found_names_the_agent_whose_login_it_means_when_the_hub_runs_one() {
    let only_claude = Agents::new(vec![claude(None)]);

    assert!(words::none_found(&only_claude).contains("Claude Code's own login shows here"));
    assert!(words::found_says(&only_claude).starts_with("The logins Claude Code already had"));
    let unknown = AgentDescription { kind: AgentKind::Unknown, ..claude(None) };
    let with_unknown = Agents::new(vec![claude(None), unknown]);
    assert!(words::none_found(&with_unknown).contains("Claude Code's"), "one this build can't name isn't one");
    assert!(words::none_found(&Agents::default()).contains("An agent's own login shows here"));
}
