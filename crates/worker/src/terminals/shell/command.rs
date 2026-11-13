//! What a terminal's shell is started as, and the environment it gets.

use std::ffi::OsStr;

use portable_pty::CommandBuilder;
use process::{Environment, TakesEnvironment};

/// What the shell's programs are told the terminal is. Every system knows it, unlike Ghostty's own name, which needs a
/// description of its own installed, and Ghostty's terminal core does what it describes.
const TERM: &str = "xterm-256color";
/// Tells programs the terminal takes any colour, as Ghostty's core does, which `TERM` can't say, so a program that
/// checks, such as an editor's theme, uses its own colours rather than the nearest of 256.
const COLORTERM: &str = "truecolor";
/// What a terminal app tells the programs it runs about itself, which a hub started from one passes on, and which
/// would tell a program here that it runs in that app, as `TMUX` makes tmux refuse to start in itself. So do names
/// that start with one of [`OTHER_TERMINALS_PREFIXES`].
const OTHER_TERMINALS_ENV: &[&str] = &[
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "TERMINFO",
    "TERMINAL_EMULATOR",
    "COLORFGBG",
    "LC_TERMINAL",
    "LC_TERMINAL_VERSION",
    "TMUX",
    "TMUX_PANE",
    "STY",
    "WINDOW",
    "WINDOWID",
    "VTE_VERSION",
    "KONSOLE_VERSION",
    "WT_SESSION",
    "WT_PROFILE_ID",
];
const OTHER_TERMINALS_PREFIXES: &[&str] = &["GHOSTTY_", "KITTY_", "ITERM_", "WEZTERM_", "ALACRITTY_", "KONSOLE_DBUS_"];

/// The user's own shell, as a login shell, as a terminal app starts it, so it reads their profile, in `cwd`, with `env`
/// set on top of the worker's own, and then what every process the worker starts is `given`, such as its git
/// credential, last, as on every other. On Windows it's `cmd.exe`, or whatever `ComSpec` names.
pub(super) fn shell(cwd: &str, env: Vec<(String, String)>, given: &Environment) -> CommandBuilder {
    let mut command = CommandBuilder::new_default_prog();
    command.cwd(cwd);
    for (name, value) in env {
        command.env(name, value);
    }
    // As an agent doesn't get them, since a hub started from inside a Claude session has them, and the CLI run here
    // would take itself for part of that session.
    for name in agents::claude::SCRUBBED_ENV {
        command.env_remove(name);
    }
    let others: Vec<String> = command.iter_full_env_as_str().map(|(name, _)| name.to_string()).collect();
    for name in others.into_iter().filter(|name| another_terminals(name)) {
        command.env_remove(name);
    }
    command.env("TERM", TERM);
    command.env("COLORTERM", COLORTERM);
    given.apply(&mut Shell(&mut command));
    command
}

/// The shell's command, as what an [`Environment`] goes on. It's wrapped, since neither the trait nor portable-pty's
/// command is this crate's.
struct Shell<'a>(&'a mut CommandBuilder);

impl TakesEnvironment for Shell<'_> {
    fn take_out(&mut self, name: &OsStr) {
        self.0.env_remove(name);
    }

    fn give(&mut self, name: &OsStr, value: &OsStr) {
        self.0.env(name, value);
    }
}

fn another_terminals(name: &str) -> bool {
    OTHER_TERMINALS_ENV.contains(&name) || OTHER_TERMINALS_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}
