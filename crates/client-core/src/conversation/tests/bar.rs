//! What the bar shows, on the device that has the conversation and on another.

use protocol::capability::output::{CoverLine, CoverWait};
use protocol::fixtures::session;
use protocol::model::{ClientDevice, DeviceId, Interaction, ReplyPlace, Session, SessionStatus, SpokenThrough};
use protocol::tail::{Covering, Tail};

use crate::conversation::{Bar, HereNow, Needs, Readiness, Shows, Stage};

fn studio() -> ClientDevice {
    ClientDevice { id: "studio".into(), name: "Mac Studio".into() }
}

fn air() -> DeviceId {
    "air".into()
}

fn on(device: ClientDevice, status: SessionStatus) -> Session {
    Session { interaction: Interaction::Conversation { device: Box::new(device) }, status, ..session("s") }
}

fn idle() -> HereNow {
    HereNow { hearing: false, reading: false, needs: Needs::READY }
}

fn covered(line: &str) -> Tail {
    let line = CoverLine { wait: CoverWait(1), number: 1, text: line.into() };
    Tail { covering: Some(Box::new(Covering { run: None, line })), ..Tail::default() }
}

/// What `bar` shows once it has seen `session`, as a chat's record comes in before it's drawn.
fn shows(bar: &mut Bar, session: &Session, tail: &Tail, this: Option<&DeviceId>, now: &HereNow) -> Shows {
    bar.saw(session, this);
    bar.shows(session, tail, this, now)
}

fn here(stage: Stage) -> Shows {
    Shows::Here { stage, needs: Needs::READY }
}

/// On the device that has it, hearing comes first, then the voice, then the agent at work with its line.
#[test]
fn here_the_bar_says_what_the_conversation_is_at() {
    let mut bar = Bar::default();
    let this = studio().id;
    let working = on(studio(), SessionStatus::Running);
    let line = covered("Hold on, one sec.");

    assert_eq!(shows(&mut bar, &session("s"), &Tail::default(), Some(&this), &idle()), Shows::Composer);
    assert_eq!(shows(&mut bar, &on(studio(), SessionStatus::Idle), &line, Some(&this), &idle()), here(Stage::Waiting));
    assert_eq!(shows(&mut bar, &working, &Tail::default(), Some(&this), &idle()), here(Stage::Working { line: None }));
    assert_eq!(
        shows(&mut bar, &working, &line, Some(&this), &idle()),
        here(Stage::Working { line: Some("Hold on, one sec.".into()) })
    );
    let reading = HereNow { reading: true, ..idle() };
    assert_eq!(shows(&mut bar, &working, &line, Some(&this), &reading), here(Stage::Speaking));
    let hearing = HereNow { hearing: true, ..reading.clone() };
    assert_eq!(shows(&mut bar, &working, &line, Some(&this), &hearing), here(Stage::Hearing));

    let needs = Needs { dictation: Readiness::NotOn, speech: Readiness::Ready };
    let not_ready = HereNow { needs: needs.clone(), ..reading };
    assert_eq!(
        shows(&mut bar, &working, &line, Some(&this), &not_ready),
        Shows::Here { stage: Stage::Waiting, needs },
        "until both are ready, it waits for them, whatever the agent does"
    );
}

/// Another device says where the conversation is, with what it's at there when the chat says, and the device it
/// left says where it went, until the mode ends.
#[test]
fn elsewhere_the_bar_says_where_the_conversation_is_or_went() {
    let mut bar = Bar::default();
    let elsewhere = |stage| Shows::Elsewhere { device: "Mac Studio".into(), stage };
    let speaking = SpokenThrough { message: "m1".into(), through: ReplyPlace { block: 0, through: 4 } };

    assert_eq!(
        shows(&mut bar, &on(studio(), SessionStatus::Idle), &Tail::default(), Some(&air()), &idle()),
        elsewhere(None)
    );
    let working = on(studio(), SessionStatus::Running);
    assert_eq!(
        shows(&mut bar, &working, &covered("Hold on."), Some(&air()), &idle()),
        elsewhere(Some(Stage::Working { line: Some("Hold on.".into()) }))
    );
    let read = Tail { speaking: Some(speaking), ..Tail::default() };
    assert_eq!(shows(&mut bar, &working, &read, Some(&air()), &idle()), elsewhere(Some(Stage::Speaking)));

    let air_device = ClientDevice { id: air(), name: "Ada's MacBook Air".into() };
    shows(&mut bar, &on(air_device, SessionStatus::Idle), &Tail::default(), Some(&air()), &idle());
    let moved = Shows::MovedTo { device: "Mac Studio".into() };
    assert_eq!(shows(&mut bar, &on(studio(), SessionStatus::Idle), &Tail::default(), Some(&air()), &idle()), moved);

    shows(&mut bar, &session("s"), &Tail::default(), Some(&air()), &idle());
    assert_eq!(
        shows(&mut bar, &on(studio(), SessionStatus::Idle), &Tail::default(), Some(&air()), &idle()),
        elsewhere(None),
        "once the mode has ended, it's no longer where it went"
    );
}

/// The words every client says the bar with, and where it says them its own way.
#[test]
fn the_bar_says_where_the_conversation_stands_in_the_same_words_everywhere() {
    let says = |shows: Shows| (shows.says(), shows.on());
    let some = |words: &str| Some(words.to_string());
    let elsewhere = |stage| Shows::Elsewhere { device: "Mac Studio".into(), stage };
    let line = || Stage::Working { line: Some("Hold on, one sec.".into()) };
    let waiting = |dictation, speech| Shows::Here { stage: Stage::Waiting, needs: Needs { dictation, speech } };

    assert_eq!(says(here(Stage::Waiting)), (None, None), "each says how to talk its own way");
    assert_eq!(says(here(Stage::Hearing)), (None, None), "what's heard is the hearing's own");
    assert_eq!(says(here(Stage::Working { line: None })), (None, None));
    assert_eq!(says(here(line())), (some("Hold on, one sec."), None));
    assert_eq!(says(here(Stage::Speaking)), (some("Speaking"), None));
    let speech = |speech| says(waiting(Readiness::Ready, speech));
    assert_eq!(speech(Readiness::Downloading(None)), (some("Getting speech ready"), None));
    assert_eq!(speech(Readiness::Downloading(Some(0.404))), (some("Getting speech ready, 40%"), None));
    let failed = speech(Readiness::Failed("Hugging Face answered 404.".into()));
    assert_eq!(failed, (some("Couldn't get speech ready: Hugging Face answered 404."), None));

    assert_eq!(says(elsewhere(None)), (some("The conversation is on Mac Studio"), None));
    assert_eq!(says(elsewhere(Some(Stage::Working { line: None }))), (some("The conversation is on Mac Studio"), None));
    assert_eq!(says(elsewhere(Some(line()))), (some("Hold on, one sec."), some("On Mac Studio")));
    assert_eq!(says(elsewhere(Some(Stage::Speaking))), (some("Speaking"), some("On Mac Studio")));
    let moved = Shows::MovedTo { device: "Mac Studio".into() };
    assert_eq!(says(moved), (some("The conversation moved to Mac Studio"), None));
    assert_eq!(says(Shows::Composer), (None, None));
}

/// While the conversation is here and dictation or speech isn't ready, the bar says what's missing: what failed, then
/// what isn't on, then what's getting ready.
#[test]
fn the_bar_says_what_the_conversation_still_needs() {
    let says = |dictation, speech| Needs { dictation, speech }.says();
    let some = |words: &str| Some(words.to_string());
    let failed = || Readiness::Failed("Hugging Face answered 404.".into());

    assert_eq!(says(Readiness::Ready, Readiness::Ready), None);
    assert_eq!(says(Readiness::NotOn, Readiness::Ready), some("Turn on dictation to talk"));
    assert_eq!(says(Readiness::Ready, Readiness::NotOn), some("Turn on speech to hear replies"));
    assert_eq!(says(Readiness::NotOn, Readiness::NotOn), some("Turn on dictation and speech to talk"));
    assert_eq!(says(Readiness::Preparing, Readiness::Ready), some("Getting dictation ready"));
    assert_eq!(says(Readiness::Downloading(Some(0.5)), Readiness::Ready), some("Getting dictation ready, 50%"));
    let both = says(Readiness::Downloading(Some(0.5)), Readiness::Downloading(None));
    assert_eq!(both, some("Getting dictation and speech ready"));
    assert_eq!(
        says(Readiness::Preparing, Readiness::NotOn),
        some("Turn on speech to hear replies"),
        "what isn't on first"
    );
    let failed_first = says(Readiness::NotOn, failed());
    assert_eq!(failed_first, some("Couldn't get speech ready: Hugging Face answered 404."), "what failed first");
}

/// The card that sets up the mode lists how each stands, and its button names what it turns on.
#[test]
fn the_card_says_how_each_stands_and_what_turning_on_turns_on() {
    let turn_on = |dictation, speech| Needs { dictation, speech }.turn_on();
    assert_eq!(turn_on(Readiness::NotOn, Readiness::Ready).as_deref(), Some("Turn on dictation"));
    assert_eq!(turn_on(Readiness::Preparing, Readiness::NotOn).as_deref(), Some("Turn on speech"));
    assert_eq!(turn_on(Readiness::NotOn, Readiness::NotOn).as_deref(), Some("Turn on both"));
    assert_eq!(turn_on(Readiness::Downloading(None), Readiness::Ready), None);

    let rows = [
        (Readiness::NotOn, "Not on"),
        (Readiness::Downloading(None), "Downloading"),
        (Readiness::Downloading(Some(0.404)), "Downloading, 40%"),
        (Readiness::Preparing, "Preparing"),
        (Readiness::Ready, "Ready"),
        (Readiness::Failed("Hugging Face answered 404.".into()), "Couldn't get it ready: Hugging Face answered 404."),
    ];
    for (readiness, says) in rows {
        assert_eq!(readiness.says(), says);
    }
}
