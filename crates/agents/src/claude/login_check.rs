//! Check the actual invocation before handing it a prompt. Settings replies are consumed here,
//! before any transcript or conformance tap can see their credentials.

use std::collections::VecDeque;
use std::time::Duration;

use protocol::model::AgentLogin;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use crate::AgentError;
use crate::stdio::next_line;

const PREFIX: &str = "roundhouse-login-";
const TIMEOUT: Duration = Duration::from_secs(20);

/// Retains ordinary startup output and any bytes read ahead during the check.
pub(super) struct Output {
    pub(super) reader: BufReader<ChildStdout>,
    pending: VecDeque<String>,
}

impl Output {
    pub(super) fn new(stdout: ChildStdout) -> Self {
        Self { reader: BufReader::new(stdout), pending: VecDeque::new() }
    }

    pub(super) async fn read_all(&mut self, bytes: &mut Vec<u8>) -> std::io::Result<usize> {
        for line in self.pending.drain(..) {
            bytes.extend_from_slice(line.as_bytes());
            bytes.push(b'\n');
        }
        self.reader.read_to_end(bytes).await
    }

    pub(super) async fn next(&mut self, buffer: &mut Vec<u8>) -> Option<String> {
        if let Some(line) = self.pending.pop_front() {
            return Some(line);
        }
        next_line(&mut self.reader, buffer).await
    }
}

pub(super) async fn verify(stdin: &mut ChildStdin, output: &mut Output, login: &AgentLogin) -> Result<(), AgentError> {
    tokio::time::timeout(TIMEOUT, async {
        let settings = pinned(login);
        ask(stdin, output, json!({"subtype":"apply_flag_settings", "settings":settings})).await?;
        let initialized = ask(stdin, output, json!({"subtype":"initialize"})).await?;
        check_account(&initialized)?;
        let settings = ask(stdin, output, json!({"subtype":"get_settings"})).await?;
        check_settings(&settings, login)
    })
    .await
    .map_err(|_| refused("the CLI did not confirm its login in time"))?
}

async fn ask(stdin: &mut ChildStdin, output: &mut Output, request: Value) -> Result<Value, AgentError> {
    let id = format!("{PREFIX}{}", request["subtype"].as_str().unwrap_or_default());
    let request = json!({"type":"control_request", "request_id":id, "request":request});
    stdin.write_all(format!("{request}\n").as_bytes()).await?;
    stdin.flush().await?;
    let mut buffer = Vec::new();
    let mut held_bytes = 0;
    while let Some(line) = next_line(&mut output.reader, &mut buffer).await {
        let value = serde_json::from_str::<Value>(&line).unwrap_or(Value::Null);
        if value["type"] == "control_response" && value["response"]["request_id"] == id {
            let reply = &value["response"];
            if reply["subtype"] != "success" {
                return Err(refused("the CLI cannot confirm its login"));
            }
            return Ok(reply["response"].clone());
        }
        held_bytes += line.len();
        if held_bytes > 1024 * 1024 {
            return Err(refused("too much output before login confirmation"));
        }
        output.pending.push_back(line);
    }
    Err(refused("the CLI exited before confirming its login"))
}

fn refused(reason: &str) -> AgentError {
    AgentError::Refused(format!("Cannot use the selected account: {reason}. Check Claude settings or update the CLI."))
}

fn check_account(reply: &Value) -> Result<(), AgentError> {
    let account = &reply["account"];
    if account["tokenSource"] != "CLAUDE_CODE_OAUTH_TOKEN"
        || account["apiProvider"] != "firstParty"
        || account.get("apiKeySource").is_some_and(|source| !source.is_null())
    {
        return Err(refused("another credential or provider overrides it"));
    }
    Ok(())
}

/// Flag settings have precedence over reloaded user/project settings. The token travels only on stdin.
fn credentials(login: &AgentLogin) -> serde_json::Map<String, Value> {
    let mut env = serde_json::Map::new();
    for name in super::login::OUTRANKS_THE_TOKEN {
        env.insert((*name).to_string(), json!(""));
    }
    env.insert("ANTHROPIC_BASE_URL".into(), json!("https://api.anthropic.com"));
    env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), json!(login.token.as_str()));
    env
}

fn pinned(login: &AgentLogin) -> Value {
    json!({"apiKeyHelper":"", "env":credentials(login)})
}

fn check_settings(reply: &Value, login: &AgentLogin) -> Result<(), AgentError> {
    let effective = reply["effective"].as_object().ok_or_else(|| refused("the CLI did not report its settings"))?;
    if effective.get("apiKeyHelper").and_then(Value::as_str) != Some("") {
        return Err(refused("apiKeyHelper could not be disabled"));
    }
    let env = effective
        .get("env")
        .and_then(Value::as_object)
        .ok_or_else(|| refused("the CLI did not confirm its credential settings"))?;
    for (name, value) in credentials(login) {
        if env.get(&name) != Some(&value) {
            return Err(refused(&format!("{name} could not be pinned to the selected account")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::model::LoginToken;

    #[test]
    fn identity_and_settings_must_both_confirm_the_selected_login() {
        let account = json!({"account":{"tokenSource":"CLAUDE_CODE_OAUTH_TOKEN","apiProvider":"firstParty"}});
        assert!(check_account(&account).is_ok());
        for bad in [
            Value::Null,
            json!({"account":{"tokenSource":"other","apiProvider":"firstParty"}}),
            json!({"account":{"tokenSource":"CLAUDE_CODE_OAUTH_TOKEN","apiProvider":"vertex"}}),
        ] {
            assert!(check_account(&bad).is_err());
        }
        let login = AgentLogin { token: LoginToken::new("selected") };
        assert!(check_settings(&json!({"effective":pinned(&login)}), &login).is_ok());
        for settings in [
            Value::Null,
            json!({"effective":{"apiKeyHelper":"sentinel-secret"}}),
            json!({"effective":{"env":{"ANTHROPIC_BASE_URL":"sentinel-secret"}}}),
            json!({"effective":{"env":{"CLAUDE_CODE_OAUTH_TOKEN":"sentinel-secret"}}}),
        ] {
            let error = check_settings(&settings, &login).unwrap_err().to_string();
            assert!(!error.contains("sentinel-secret"));
        }
    }
}
