use super::super::automatic::{Candidate, choose};
use protocol::fixtures::account;
use protocol::model::{Account, AccountId, AgentKind, HeldLogin, LimitWindow, Reserve, UsageLimit};

const NOW: i64 = 1_000_000_000;
const HOUR: i64 = 60 * 60 * 1000;

/// A login added for Claude, called `id`, that works, with `limits`.
fn added(id: &str, limits: Vec<UsageLimit>) -> Account {
    let held = HeldLogin { connected_at_ms: 0, ends_at_ms: None };
    Account { login_agent: Some(AgentKind::Claude), limits, held_login: Some(held), ..account(id) }
}

/// A limit over `window` with `left` percent left, which starts again `hours` from now.
fn limit(window: LimitWindow, left: u8, hours: i64) -> UsageLimit {
    UsageLimit {
        window,
        model: None,
        used_percent: 100 - left,
        resets_at_ms: Some(NOW + hours * HOUR),
        heard_at_ms: None,
    }
}

fn week(left: u8, hours: i64) -> UsageLimit {
    limit(LimitWindow::SevenDays, left, hours)
}

/// Which of `accounts`, each with the chats at work on it, Automatic chooses.
fn chosen(accounts: &[(Account, usize)]) -> Option<AccountId> {
    let candidates: Vec<Candidate<'_>> =
        accounts.iter().map(|(account, working)| Candidate { account, working: *working }).collect();
    choose(AgentKind::Claude, &candidates, NOW).cloned()
}

fn id(name: &str) -> AccountId {
    AccountId::from(name)
}

#[test]
fn the_account_with_the_most_of_its_week_to_spare_for_each_hour_left_is_chosen() {
    let soon = added("soon", vec![week(40, 24)]);
    let later = added("later", vec![week(60, 144)]);
    let unheard = added("unheard", Vec::new());
    let started_again = added("started-again", vec![week(5, -1)]);

    assert_eq!(chosen(&[(later.clone(), 0), (soon.clone(), 0)]), Some(id("soon")), "40% for a day beats 60% for six");
    assert_eq!(chosen(&[(later.clone(), 0), (unheard, 0)]), Some(id("unheard")), "all of a week no agent has heard");
    assert_eq!(
        chosen(&[(later.clone(), 0), (started_again, 0)]),
        Some(id("started-again")),
        "all of a week started again"
    );
    let reserving = Account { reserve: Reserve::new(35), ..soon.clone() };
    assert_eq!(chosen(&[(reserving, 0), (later.clone(), 0)]), Some(id("later")), "5% beyond the reserve, for a day");
    assert_eq!(chosen(&[(soon.clone(), 4), (later.clone(), 0)]), Some(id("later")), "a day's 40% shared by five chats");
    let twin = Account { id: "twin".into(), ..later.clone() };
    assert_eq!(chosen(&[(later.clone(), 1), (twin.clone(), 0)]), Some(id("twin")), "the same to spare, fewer at work");
    assert_eq!(chosen(&[(twin, 0), (later.clone(), 0)]), Some(id("later")), "the same again goes to the first by ID");
    let last_minutes = added("last-minutes", vec![UsageLimit { resets_at_ms: Some(NOW + HOUR / 10), ..week(10, 0) }]);
    let two_hours = added("two-hours", vec![week(50, 2)]);
    assert_eq!(
        chosen(&[(last_minutes, 0), (two_hours, 0)]),
        Some(id("two-hours")),
        "a week's last minutes count as an hour"
    );
}

#[test]
fn an_account_with_nothing_to_spare_or_whose_login_cant_be_used_is_never_chosen() {
    let out = added("out", vec![limit(LimitWindow::FiveHours, 0, 2), week(90, 100)]);
    let in_reserve = Account { reserve: Reserve::new(50), ..added("in-reserve", vec![week(50, 24)]) };
    let all_reserved = Account { reserve: Some(Reserve::ALL), ..added("all-reserved", Vec::new()) };
    let ended = Account {
        held_login: Some(HeldLogin { connected_at_ms: 0, ends_at_ms: Some(NOW) }),
        ..added("ended", Vec::new())
    };
    let disconnected = Account { held_login: None, ..added("disconnected", Vec::new()) };
    let found = Account { login_agent: None, ..added("found", Vec::new()) };
    let codex = Account { login_agent: Some(AgentKind::Codex), ..added("codex", Vec::new()) };
    let none: Vec<(Account, usize)> =
        [out, in_reserve, all_reserved, ended, disconnected, found, codex].into_iter().map(|a| (a, 0)).collect();

    assert_eq!(chosen(&none), None);
    let out_again = added("out-again", vec![limit(LimitWindow::FiveHours, 0, -1), week(1, 100)]);
    assert_eq!(chosen(&[(out_again, 0)]), Some(id("out-again")), "a limit used up that has started again since");
}
