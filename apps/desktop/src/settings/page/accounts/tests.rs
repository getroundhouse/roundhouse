use std::time::{Duration, Instant};

use client_core::stand_in_hub::{self, Does};
use client_core::{Agents, ClientError, RecordsFeed};
use gpui_kit::{AppContext as _, Context, IntoElement, Render, TestAppContext, Window};
use protocol::client::{ApiError, ClientRequest, ErrorCode, ReplyOutcome};
use protocol::fixtures::account;
use protocol::model::{
    Account, AccountId, AgentDescription, AgentKind, Effort, HeldLogin, LimitWindow, LoginDescription, ModelName,
    PermissionMode, SessionSettings, UsageLimit,
};

use super::Asking;
use super::kept::{Kept, To};
use super::row::changes_at_ms;
use super::words::{self, Refused, Standing};
use super::{listed, older_hub};

/// Sunday 1 November 2026, at 3 PM in UTC, which held still is the local time zone.
const NOW_MS: i64 = 1_793_545_200_000;
const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 24 * 60 * MINUTE_MS;

/// An account whose login the person gave the hub, called `name`, which the hub holds.
fn added(id: &str, name: &str) -> Account {
    Account {
        name: Some(name.to_string()),
        login_agent: Some(AgentKind::Claude),
        held_login: Some(HeldLogin { connected_at_ms: 0, ends_at_ms: None }),
        ..account(id)
    }
}

#[test]
fn an_added_account_is_called_by_its_name_and_a_found_one_by_its_email() {
    let both = |account: Account| Account { email: Some("ada@example.com".to_string()), ..account };

    assert_eq!(words::called(&both(added("a", "Work"))), "Work");
    assert_eq!(words::called(&both(Account { name: Some("Work".to_string()), ..account("a") })), "ada@example.com");
    assert_eq!(words::called(&Account { login: Some("API key".to_string()), ..account("a") }), "API key");
    assert_eq!(words::called(&account("claude-1")), "claude-1", "with nothing else to go by");
}

#[test]
fn found_and_added_are_apart_each_by_what_its_row_calls_it_with_those_disconnected_last() {
    let found = Account { email: Some("bea@example.com".to_string()), ..account("f1") };
    let gone = Account { held_login: None, ..added("a3", "Archive") };
    let accounts = [added("a1", "work"), found.clone(), gone, added("a2", "Contractor"), account("f2")];

    let (found, added) = listed(accounts.iter());

    let ids = |list: Vec<&Account>| list.iter().map(|account| account.id.to_string()).collect::<Vec<_>>();
    assert_eq!(ids(found), ["f1", "f2"], "bea@example.com, then the one called by its ID");
    assert_eq!(ids(added), ["a2", "a1", "a3"], "in any case, and Archive, disconnected, after them");
}

/// Claude as a hub describes it, with what makes a token for it, as `login` says, if anything.
fn claude(login: Option<LoginDescription>) -> AgentDescription {
    AgentDescription {
        capabilities: None,
        kind: AgentKind::Claude,
        name: "Claude Code".to_string(),
        usage_heading: "Claude usage".to_string(),
        models: Vec::new(),
        efforts: Vec::new(),
        modes: Vec::new(),
        defaults: SessionSettings {
            model: ModelName::named("opus"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::Xhigh,
        },
        features: Vec::new(),
        login,
    }
}

fn setup_token() -> LoginDescription {
    LoginDescription { made_with: "claude setup-token".to_string(), lasts_ms: None }
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
    assert_eq!(standing(cx, Some(NOW_MS + 345 * DAY_MS)), Standing::Held(Some("Ends Oct 12, 2027".to_string())));
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

#[gpui_kit::test]
fn disconnecting_with_no_hub_to_ask_says_so_under_that_accounts_row_alone(cx: &mut TestAppContext) {
    let kept = cx.new(|_| Kept::default());

    kept.update(cx, |kept, cx| kept.disconnect(AccountId::from("a"), cx));

    kept.read_with(cx, |kept, _| {
        assert_eq!(kept.not_disconnected(&AccountId::from("a")), Some(words::NO_HUB_TO_DISCONNECT));
        assert_eq!(kept.not_disconnected(&AccountId::from("b")), None);
    });
}

#[test]
fn the_rows_are_drawn_again_as_what_they_say_of_a_time_changes() {
    let midnight_ms = Some(NOW_MS + 9 * 60 * MINUTE_MS);
    let changes = |accounts: &[&Account]| changes_at_ms(accounts.iter().copied(), NOW_MS, midnight_ms);
    let used = Account { used_at_ms: Some(NOW_MS - 125 * MINUTE_MS), ..account("a") };
    let held = |ends_at_ms| Account {
        held_login: Some(HeldLogin { connected_at_ms: 0, ends_at_ms: Some(ends_at_ms) }),
        ..added("b", "Work")
    };

    assert_eq!(changes(&[&used]), midnight_ms, "as it was yesterday");
    assert_eq!(
        changes(&[&held(NOW_MS + 30 * DAY_MS + 3 * 60 * MINUTE_MS)]),
        Some(NOW_MS + 3 * 60 * MINUTE_MS),
        "as it warns"
    );
    let ends_at_ms = NOW_MS + 3 * 60 * MINUTE_MS;
    let lasting_a_day = Account {
        held_login: Some(HeldLogin { connected_at_ms: ends_at_ms - DAY_MS, ends_at_ms: Some(ends_at_ms) }),
        ..added("d", "Day")
    };
    assert_eq!(
        changes(&[&lasting_a_day]),
        Some(NOW_MS + 60 * MINUTE_MS),
        "as one that lasts a day warns, 2 hours before"
    );
    assert_eq!(changes(&[&used, &held(NOW_MS + 10 * MINUTE_MS)]), Some(NOW_MS + 10 * MINUTE_MS), "as it ends");
    assert_eq!(changes(&[&held(NOW_MS - 1), &account("c")]), None, "once it has ended, nothing changes");
    let limit = |resets_at_ms| UsageLimit {
        window: LimitWindow::FiveHours,
        model: None,
        used_percent: 40,
        resets_at_ms: Some(resets_at_ms),
        heard_at_ms: None,
    };
    let limited = Account { limits: vec![limit(NOW_MS - 1), limit(NOW_MS + 5 * MINUTE_MS)], ..account("d") };
    assert_eq!(changes(&[&limited]), Some(NOW_MS + 5 * MINUTE_MS), "as a limit shown starts again");
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

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// Against a stand-in hub that says what makes a login, as `Kept` is drawn with, and refuses to connect one as too
/// old to, as a hub whose description runs ahead of what it can do would.
#[cfg(unix)]
#[gpui_kit::test]
fn a_hub_that_refuses_to_connect_a_login_as_too_old_is_taken_for_one_from_then_on(cx: &mut TestAppContext) {
    // The hub answers on a real thread.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), |request| match request {
        ClientRequest::AccountConnect(_) => {
            Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "unknown request"))))
        }
        _ => Does::Nothing,
    });
    let asking = Asking { feed: RecordsFeed::start(&client), client };
    let window = cx.add_window(|_, _| Blank);
    let kept = cx.new(|_| Kept::default());
    window
        .update(cx, |_, window, cx| {
            let settings = cx.focus_handle();
            kept.update(cx, |kept, cx| {
                kept.drawn(Some(asking), settings, None, cx);
                kept.open_new(AgentKind::Claude, window, cx);
                let panel = kept.panel.as_ref().unwrap();
                let To::New { name } = &panel.to else { unreachable!("a new account's panel") };
                name.input.update(cx, |name, cx| name.set_value("Work", window, cx));
                panel.token.input.update(cx, |token, cx| token.set_value("a-made-up-token", window, cx));
                kept.connect(window, cx);
            });
        })
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(20);
    while !kept.read_with(cx, |kept, _| kept.refused_as_older) {
        assert!(Instant::now() < deadline, "timed out waiting for the refusal");
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
    kept.read_with(cx, |kept, _| assert!(kept.panel.is_none(), "the panel goes, since nothing can be connected"));
}
