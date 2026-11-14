//! Running the CLI under a login its hub holds, rather than the one on this machine. `docs/claude-cli.md` has what the
//! CLI does with one.

use protocol::model::AgentLogin;
use tokio::process::Command;

/// Where the CLI reads a token `claude setup-token` made. A hook the CLI started didn't have it in its environment,
/// with nothing set to ask for that. Whether a Bash call's has it wasn't seen.
const TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// What the CLI would use before that token, by its own order: another provider's credentials, a bearer token and an
/// API key. A machine that sets one would otherwise run the agent under it, whatever login its hub chose. Each was
/// seen to take over from a token, in what `auth status` says.
pub(super) const OUTRANKS_THE_TOKEN: &[&str] = &[
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_MANTLE",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_CUSTOM_HEADERS",
];

/// Where the CLI sends its requests instead of Anthropic's API, which would get the token with them.
const SENDS_IT_ELSEWHERE: &str = "ANTHROPIC_BASE_URL";

/// Has `command` run the CLI under `login`, at Anthropic's own API, and under nothing that would outrank it. With
/// none, the command is left as it is, so the CLI uses this machine's own login. [`start`](super::process::start)
/// calls it just before the CLI starts, so nothing set after it puts back what this takes out. The stream startup then pins these settings in the CLI's memory and verifies them with `login_check`,
/// since settings can otherwise put credentials back after the process starts.
pub(in crate::claude) fn run_under(command: &mut Command, login: Option<&AgentLogin>) {
    let Some(login) = login else { return };
    for name in OUTRANKS_THE_TOKEN.iter().chain([&SENDS_IT_ELSEWHERE]) {
        command.env_remove(name);
    }
    command.env(TOKEN_ENV, login.token.as_str());
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use protocol::model::LoginToken;

    use super::*;

    /// What `command` gives the CLI as `name`, if it sets it.
    fn given<'a>(command: &'a Command, name: &str) -> Option<&'a OsStr> {
        command.as_std().get_envs().find(|(set, _)| *set == OsStr::new(name)).and_then(|(_, value)| value)
    }

    /// Whether `command` takes `name` out of the environment the CLI would inherit.
    fn taken_out(command: &Command, name: &str) -> bool {
        command.as_std().get_envs().any(|(set, value)| set == OsStr::new(name) && value.is_none())
    }

    #[test]
    fn a_login_sets_the_token_and_takes_out_what_would_outrank_it() {
        let mut command = Command::new("claude");
        command.env("ANTHROPIC_API_KEY", "the-machines-own");

        run_under(&mut command, Some(&AgentLogin { token: LoginToken::new("a-made-up-token") }));

        assert_eq!(given(&command, TOKEN_ENV), Some(OsStr::new("a-made-up-token")));
        for name in OUTRANKS_THE_TOKEN.iter().chain([&SENDS_IT_ELSEWHERE]) {
            assert!(taken_out(&command, name), "{name}");
        }
    }

    #[test]
    fn without_a_login_the_machines_own_is_left_alone() {
        let mut command = Command::new("claude");
        command.env("ANTHROPIC_API_KEY", "the-machines-own");

        run_under(&mut command, None);

        assert_eq!(given(&command, TOKEN_ENV), None);
        assert!(!taken_out(&command, TOKEN_ENV));
        assert_eq!(given(&command, "ANTHROPIC_API_KEY"), Some(OsStr::new("the-machines-own")));
    }
}
