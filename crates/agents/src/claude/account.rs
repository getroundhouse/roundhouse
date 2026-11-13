//! Whose account the CLI is logged in to, from `claude auth status --json`, which answers from what's on the machine
//! and spends nothing. `docs/claude-cli.md` has what it says. Under a login its hub holds, it says only that there's a
//! token, not whose, so the account has no ID here and its hub names it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::model::{AccountId, AgentAccount};
use serde_json::Value;

use super::one_off::{self, OneOff};
use super::process::resolve_binary;
use crate::{AgentError, AgentOptions, NoAccount};

/// How the CLI says it's logged in with a token `claude setup-token` made.
const TOKEN_LOGIN: &str = "oauth_token";

/// It answers in about a tenth of a second, so one taking this long is stuck.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The account, and which one it is. Run in `scratch_dir`, so no project's settings are read. `options.pid_file` records it while it runs, and dropping the future stops the call and everything it
/// started.
pub async fn account(options: &AgentOptions, scratch_dir: &Path) -> Result<AgentAccount, AgentError> {
    let bin = resolve_binary(&options.binary).ok_or(AgentError::NotInstalled)?;
    let pid_file: Option<PathBuf> = options.pid_file.clone();
    let args = ["auth", "status", "--json"].map(String::from).to_vec();
    let login = options.login.as_ref();
    let call = OneOff { bin: &bin, cwd: scratch_dir, args, stdin: b"", timeout: TIMEOUT, pid_file, login };
    let output = one_off::run(call).await?;
    // How it exited isn't looked at: what it printed says whether anyone is logged in.
    Ok(account_from(&output.stdout, login.is_some())?)
}

/// What the CLI's status says, in words for a person. Anything it doesn't say is left out, as a plan is for a login
/// with an API key. An answer that doesn't say whether anyone is logged in is no answer, rather than nobody.
///
/// `under_a_login` says the CLI was given a login its hub holds. An answer that it's logged in some other way is
/// refused then, since the CLI's own settings put something before the token, and the account isn't the hub's.
fn account_from(stdout: &[u8], under_a_login: bool) -> Result<AgentAccount, NoAccount> {
    let status: Value = serde_json::from_slice(stdout).map_err(|_| NoAccount::Unreadable)?;
    let logged_in = status.get("loggedIn").and_then(Value::as_bool).ok_or(NoAccount::Unreadable)?;
    let said = |key: &str| status.get(key).and_then(Value::as_str).map(str::trim).filter(|said| !said.is_empty());
    if let Some(method) = said("authMethod").filter(|method| under_a_login && *method != TOKEN_LOGIN) {
        return Err(NoAccount::Outranked { under: method.to_string() });
    }
    Ok(AgentAccount {
        id: logged_in.then(|| account_id(said("orgId"), said("email"))).flatten(),
        logged_in,
        model_provider: said("apiProvider").map(provider_name),
        plan: said("subscriptionType").map(capitalized),
        login: said("authMethod").filter(|_| logged_in).map(login_name),
        email: said("email").map(str::to_string),
    })
}

/// Which account a login is: the organization and, within it, the person, since it isn't known whether a limit is
/// the organization's or each member's, and taking two people for one would show one of them the other's. `None` for
/// a login that names neither, such as one with an API key, which can't be told from another.
///
/// It says it's Claude's, so no other kind of agent's account can be taken for it, and it's opaque, so what names the
/// account, such as a session's record, doesn't hold an email because of it. Only the account's own record says the
/// email, for a person to read. FNV-1a, written out here, so it's the same in every build:
/// the hub keeps limits under it across updates.
fn account_id(organization: Option<&str>, email: Option<&str>) -> Option<AccountId> {
    if organization.is_none() && email.is_none() {
        return None;
    }
    let named = format!("{}\n{}", organization.unwrap_or_default(), email.unwrap_or_default().to_lowercase());
    let hash = named
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3));
    Some(AccountId::from(format!("claude-{hash:016x}")))
}

/// Who serves the model. One with no name here shows as the CLI names it.
fn provider_name(provider: &str) -> String {
    match provider {
        "firstParty" => "Anthropic API",
        "bedrock" => "Amazon Bedrock",
        "vertex" => "Google Vertex AI",
        "foundry" => "Microsoft Foundry",
        other => other,
    }
    .to_string()
}

/// How it's logged in. One with no name here shows as the CLI names it.
fn login_name(method: &str) -> String {
    match method {
        "claude.ai" => "Claude account",
        TOKEN_LOGIN => "Long-lived token",
        other => other,
    }
    .to_string()
}

/// The CLI names a plan in lower case, such as `max`.
fn capitalized(name: &str) -> String {
    let mut letters = name.chars();
    letters.next().map_or_default(|first| first.to_uppercase().chain(letters).collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn read(status: &Value) -> AgentAccount {
        account_from(status.to_string().as_bytes(), false).unwrap()
    }

    fn subscription(organization: &str, email: &str) -> Value {
        json!({
            "loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty", "email": email,
            "orgId": organization, "orgName": "Ada's Organization", "subscriptionType": "max", "planTier": "max_20x",
        })
    }

    #[test]
    fn a_subscription_login_says_its_provider_plan_login_and_email() {
        let account = read(&subscription("0000", "ada@example.com"));

        assert!(account.logged_in);
        assert_eq!(account.model_provider.as_deref(), Some("Anthropic API"));
        assert_eq!(account.plan.as_deref(), Some("Max"));
        assert_eq!(account.login.as_deref(), Some("Claude account"));
        assert_eq!(account.email.as_deref(), Some("ada@example.com"));
    }

    #[test]
    fn what_the_cli_doesnt_say_is_left_out_and_a_name_unknown_here_shows_as_it_came() {
        let status = json!({"loggedIn": true, "authMethod": "api_key", "apiProvider": "elsewhere", "email": " "});

        let account = read(&status);

        assert_eq!(account.model_provider.as_deref(), Some("elsewhere"));
        assert_eq!(account.login.as_deref(), Some("api_key"));
        assert_eq!((account.plan, account.email), (None, None));
        assert_eq!(account.id, None, "a login that names nobody can't be told from another");
    }

    /// What the CLI says under a token `claude setup-token` made, as `docs/claude-cli.md` records.
    #[test]
    fn a_token_login_is_logged_in_and_says_how_but_not_who() {
        let status = json!({"loggedIn": true, "authMethod": "oauth_token", "apiProvider": "firstParty"});

        let account = account_from(status.to_string().as_bytes(), true).unwrap();

        assert!(account.logged_in);
        assert_eq!(account.login.as_deref(), Some("Long-lived token"));
        assert_eq!(account.model_provider.as_deref(), Some("Anthropic API"));
        assert_eq!((account.id, account.email, account.plan), (None, None, None), "its hub names the account");
    }

    /// Such as a machine whose own settings set an API key, which the CLI puts before a token.
    #[test]
    fn a_cli_given_a_login_that_runs_under_something_else_says_so_rather_than_whose_that_is() {
        let status = json!({"loggedIn": true, "authMethod": "api_key", "apiProvider": "firstParty"});

        let under_a_login = account_from(status.to_string().as_bytes(), true);

        assert!(
            matches!(&under_a_login, Err(NoAccount::Outranked { under }) if under == "api_key"),
            "{under_a_login:?}"
        );
        assert_eq!(read(&status).login.as_deref(), Some("api_key"), "with no login given, it's the machine's own");
    }

    #[test]
    fn nobody_logged_in_is_no_account_however_else_it_answers() {
        let status = json!({"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty", "orgId": "0000"});

        let account = read(&status);

        assert!(!account.logged_in);
        assert_eq!((account.id, account.login), (None, None));
    }

    #[test]
    fn an_answer_that_doesnt_say_whether_anyone_is_logged_in_is_no_account() {
        for answer in ["not json", "null", "{}", r#"{"error":"keychain locked"}"#, r#"{"loggedIn":"yes"}"#] {
            assert!(matches!(account_from(answer.as_bytes(), false), Err(NoAccount::Unreadable)), "{answer}");
        }
    }

    #[test]
    fn the_same_login_is_the_same_account_and_another_person_or_organization_is_another() {
        let id = |organization, email| read(&subscription(organization, email)).id.unwrap();
        let ada = id("0000", "ada@example.com");

        assert_eq!(id("0000", "Ada@Example.com"), ada, "however the email is written");
        assert_ne!(id("0000", "grace@example.com"), ada, "another person in the organization");
        assert_ne!(id("1111", "ada@example.com"), ada, "the same person in another organization");
        assert!(ada.as_str().starts_with("claude-"), "it says which kind of agent it's for: {ada}");
        assert!(!ada.as_str().contains("ada"), "and not who: {ada}");
    }

    /// The hub keeps limits under this across updates, so it can't change from one build to the next.
    #[test]
    fn an_accounts_id_is_the_same_in_every_build() {
        let id = read(&subscription("0000", "ada@example.com")).id.unwrap();

        assert_eq!(id.as_str(), "claude-fef3264f49fe736a");
    }
}
