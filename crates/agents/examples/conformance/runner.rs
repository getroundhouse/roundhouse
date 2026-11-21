//! Plays the conformance scenarios on the agent CLI the hub would find, or the one `--cli` names, through the
//! adapter, and says how each went.
//!
//! ```sh
//! cargo run -p agents --example conformance -- [--agent KIND] [--paid] [--save] [--out DIR] [--cli PATH] [SCENARIO ...]
//! ```
//!
//! Each plays against the scripted model, which is free, and with `--paid` against the real one too, under this
//! machine's login, or under `ROUNDHOUSE_CONFORMANCE_TOKEN`, a token `claude setup-token` made, with a config of its
//! own. `--save` keeps each passing scripted trial's fixture for `cargo test` to replay, and `--out` keeps every
//! scripted trial's in `DIR`, with what the scripted model was asked, to see what happened in one that failed. A
//! fixture's IDs are numbered and its times zeroed, as `docs/reference/conformance.md` says. One on the real model is never
//! kept. `docs/reference/conformance.md` says more.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a program for developers, which stops at what it can't do"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use agents::BinaryLookup;
use agents::claude::conformance::{KEPT_ENV, Live, Options, find_cli, live};
use agents::conformance::scenario::{Against, Scenario};
use agents::conformance::scenarios::ALL;
use protocol::model::{AgentKind, LoginToken};

/// Asks a run of this program to play only the scripted trials, in the environment it was started with, which holds
/// only [`KEPT_ENV`]. It's a process of its own rather than this one with its environment cleared, which Rust can't
/// do safely once threads run. And it's started with what it keeps, rather than having the sandboxed CLI start with
/// `env -i`, which would drop what the adapter itself gives the CLI, such as a login's token.
const SCRIPTED_ONLY: &str = "--scripted-only";

struct Args {
    agent: AgentKind,
    paid: bool,
    save: bool,
    scripted_only: bool,
    out: Option<PathBuf>,
    cli: Option<PathBuf>,
    /// This machine's home, which the scripted process is told of, since its environment doesn't say.
    home: Option<PathBuf>,
    only: Vec<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            agent: AgentKind::Claude,
            paid: false,
            save: false,
            scripted_only: false,
            out: None,
            cli: None,
            home: None,
            only: Vec::new(),
        }
    }
}

impl Args {
    fn parse() -> Args {
        let mut args = Args::default();
        let mut given = std::env::args().skip(1);
        while let Some(arg) = given.next() {
            match arg.as_str() {
                "--agent" => {
                    let name = given.next().expect("--agent takes claude, codex or antigravity");
                    args.agent = AgentKind::parse(&name).expect("--agent takes claude, codex or antigravity");
                }
                "--paid" => args.paid = true,
                "--save" => args.save = true,
                SCRIPTED_ONLY => args.scripted_only = true,
                "--cli" => args.cli = Some(given.next().expect("--cli takes a path").into()),
                "--home" => args.home = Some(given.next().expect("--home takes a path").into()),
                "--out" => args.out = Some(given.next().expect("--out takes a directory").into()),
                name => args.only.push(name.to_string()),
            }
        }
        args
    }

    fn scenarios(&self) -> impl Iterator<Item = &'static Scenario> {
        let unknown: Vec<_> = self.only.iter().filter(|name| !ALL.iter().any(|s| s.name == name.as_str())).collect();
        assert!(unknown.is_empty(), "no scenario is called {unknown:?}");
        ALL.iter()
            .copied()
            .filter(|scenario| self.only.is_empty() || self.only.iter().any(|name| name == scenario.name))
    }
}

#[tokio::main]
pub(super) async fn run() -> ExitCode {
    let args = Args::parse();
    if args.scripted_only {
        let held: Vec<_> = std::env::vars_os().filter(|(name, _)| !KEPT_ENV.iter().any(|kept| name == kept)).collect();
        assert!(held.is_empty(), "a scripted trial's environment holds {held:?}, and may hold only {KEPT_ENV:?}");
        let options = Options { cli: args.cli.clone().expect("--cli"), home: args.home.clone(), token: None };
        return exit(play(&args, Against::ScriptedModel, &options).await);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let lookup = BinaryLookup { explicit: args.cli.clone(), path: std::env::var_os("PATH"), home: home.clone() };
    let cli = find_agent_cli(args.agent, &lookup).expect("no agent CLI found");
    let mut scripted = Command::new(std::env::current_exe().unwrap());
    scripted.arg(SCRIPTED_ONLY).args(["--agent", args.agent.as_str()]).arg("--cli").arg(&cli).args(&args.only);
    if args.save {
        scripted.arg("--save");
    }
    if let Some(out) = &args.out {
        scripted.arg("--out").arg(out);
    }
    if let Some(home) = &home {
        scripted.arg("--home").arg(home);
    }
    let kept = std::env::vars_os().filter(|(name, _)| KEPT_ENV.iter().any(|kept| name == kept));
    let scripted = scripted.env_clear().envs(kept).status().unwrap().success();
    let token = match args.agent {
        AgentKind::Claude => std::env::var("ROUNDHOUSE_CONFORMANCE_TOKEN").ok().map(LoginToken::new),
        AgentKind::Codex | AgentKind::Antigravity | AgentKind::Unknown => None,
    };
    let options = Options { cli, home, token };
    let paid = !args.paid || play(&args, Against::RealModel, &options).await;
    exit(scripted && paid)
}

fn find_agent_cli(agent: AgentKind, lookup: &BinaryLookup) -> Option<PathBuf> {
    match agent {
        AgentKind::Claude => find_cli(lookup),
        AgentKind::Codex => agents::codex::conformance::find_cli(lookup),
        AgentKind::Antigravity => agents::antigravity::conformance::find_cli(lookup),
        AgentKind::Unknown => None,
    }
}

fn exit(passed: bool) -> ExitCode {
    if passed { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Plays each scenario against `against` that plays there, and says whether they all passed.
async fn play(args: &Args, against: Against, options: &Options) -> bool {
    let mut passed = true;
    for scenario in args.scenarios().filter(|scenario| scenario.against.contains(&against)) {
        let Live { report, fixture, requests } = play_one(args.agent, scenario, against, options).await;
        println!("{report}");
        passed &= report.passed();
        let Some(fixture) = fixture else { continue };
        let file = format!("{}.scripted.jsonl", scenario.name);
        let keep = |dir: &Path| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join(&file), &fixture).unwrap();
            println!("        saved {}", dir.join(&file).display());
        };
        if args.save && report.passed() {
            keep(&Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{}", args.agent.as_str())));
        }
        if let Some(out) = &args.out {
            keep(out);
            let asked = out.join(format!("{}.scripted.requests.json", scenario.name));
            std::fs::write(asked, serde_json::to_string_pretty(&requests).unwrap()).unwrap();
        }
    }
    passed
}

async fn play_one(agent: AgentKind, scenario: &'static Scenario, against: Against, options: &Options) -> Live {
    match agent {
        AgentKind::Claude => live(scenario, against, options).await,
        AgentKind::Codex => {
            let options = agents::codex::conformance::Options { cli: options.cli.clone(), home: options.home.clone() };
            agents::codex::conformance::live(scenario, against, &options).await
        }
        AgentKind::Antigravity => {
            let options =
                agents::antigravity::conformance::Options { cli: options.cli.clone(), home: options.home.clone() };
            agents::antigravity::conformance::live(scenario, against, &options).await
        }
        AgentKind::Unknown => unreachable!("parsed agent kind is known"),
    }
}
