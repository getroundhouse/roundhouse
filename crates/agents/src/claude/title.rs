//! A title for a prompt, from a one-off call to a small model, outside any session, so it costs the session nothing
//! and never shows in its chat. The prompt's images go with its words, since a prompt such as "how do we fix this?"
//! is often about a screenshot. See `docs/claude-cli.md`.

use std::path::Path;
use std::process::Output;
use std::time::Duration;

use protocol::attachment::NewAttachment;
use protocol::model::Title;
use serde_json::{Value, json};

use super::one_off::{self, OneOff};
use super::process::{image_block, resolve_binary, user_line};
use crate::{AgentError, AgentOptions, NoTitle};

/// Haiku answers in a second or two, and in about three with a full screenshot to read, so a call taking this long is
/// stuck. Stopping the worker cancels one sooner.
const TITLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Given an example, Haiku starts a title with a verb, as the spec's titles do, rather than with its "-ing" form. Told
/// what an image is for, it names a vague prompt from its screenshot.
const SYSTEM_PROMPT: &str = "You name tasks. Reply with only a short title for the task the user describes: two to \
     five words, in sentence case, with no quotes and no full stop, such as Fix login redirect. An image they send is \
     part of the task, usually a screenshot of what it's about, so when their words are vague, name the task from \
     what the image shows. Don't answer the task or ask about it.";

/// Runs in an empty directory of its own inside `scratch_dir`, so the CLI finds no project's instructions. Dropping
/// the future stops the call and everything it started.
pub async fn title_for(
    prompt: &str,
    images: &[NewAttachment],
    options: &AgentOptions,
    scratch_dir: &Path,
) -> Result<Title, AgentError> {
    let bin = resolve_binary(&options.binary).ok_or(AgentError::NotInstalled)?;
    let room = tempfile::Builder::new().prefix("title-").tempdir_in(scratch_dir).map_err(NoTitle::NoRoom)?;
    let mut asked = asked(prompt, images).to_string();
    asked.push('\n');
    let call = OneOff {
        bin: &bin,
        cwd: room.path(),
        args: title_args(),
        stdin: asked.as_bytes(),
        timeout: TITLE_TIMEOUT,
        pid_file: options.pid_file.clone(),
    };
    let output = one_off::run(call).await?;
    if !output.status.success() {
        return Err(NoTitle::Failed { status: output.status, said: said(&output) }.into());
    }
    Ok(title_from_output(&output.stdout)?)
}

/// What the call reads: the prompt's images, then its words, which is where Anthropic's guidance puts images, as a
/// session's prompt does. Anything that isn't an image, such as a kind from a newer hub, is left out.
fn asked(prompt: &str, images: &[NewAttachment]) -> Value {
    let images = images.iter().filter(|image| image.attachment.media_type.is_image());
    let mut content: Vec<Value> = images.map(|image| image_block(image.attachment.media_type, &image.data)).collect();
    if !prompt.trim().is_empty() || content.is_empty() {
        content.push(json!({"type": "text", "text": prompt}));
    }
    user_line(content)
}

/// No tools, MCP servers, slash commands, hooks or project settings, and nothing saved, so the call only writes a
/// title. The user's hooks are for their own sessions, such as one that announces a finished turn, so none runs. The
/// effort is named, as an agent's always is, so the user's own level doesn't apply. Haiku at High named tasks as well
/// as Sonnet at Low did, judged blind, at a seventeenth of the cost, and about as fast. It reads a `user` line, as a
/// session does, since that's the only way to give it an image, and the CLI then prints only lines too.
fn title_args() -> Vec<String> {
    [
        "-p",
        "--model",
        "haiku",
        "--effort",
        "high",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "user",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--no-session-persistence",
        "--disable-slash-commands",
        "--system-prompt",
        SYSTEM_PROMPT,
    ]
    .map(String::from)
    .to_vec()
}

/// What the CLI said about failing: what its `result` line says, or else its stderr. Not being logged in exits with an
/// error and says so only in that line.
fn said(output: &Output) -> String {
    let reason = result_line(&output.stdout).and_then(|value| reason(&value));
    reason.unwrap_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_string())
}

/// The `result` of the CLI's `result` line, tidied.
fn title_from_output(stdout: &[u8]) -> Result<Title, NoTitle> {
    let value = result_line(stdout).ok_or(NoTitle::NoResult)?;
    if value["is_error"].as_bool() != Some(false) {
        return Err(NoTitle::Reported(reason(&value).unwrap_or_else(|| "an error".to_string())));
    }
    tidy(value["result"].as_str().unwrap_or_default()).ok_or(NoTitle::Empty)
}

/// The line that ends the call, with its answer, or how it failed. The CLI prints a line or two after it.
fn result_line(stdout: &[u8]) -> Option<Value> {
    let lines = stdout.split(|byte| *byte == b'\n').rev();
    lines.filter_map(|line| serde_json::from_slice::<Value>(line).ok()).find(|value| value["type"] == "result")
}

/// Why the CLI's `result` line says it failed: its `result`, such as "Not logged in", or else the kind of error, since
/// an error can come with the subtype `success`.
fn reason(value: &Value) -> Option<String> {
    let result = value["result"].as_str().map(str::trim).filter(|result| !result.is_empty());
    result.or_else(|| value["subtype"].as_str()).map(str::to_string)
}

/// The first line, without the markup, label, quotes or full stop a model sometimes adds anyway.
fn tidy(answer: &str) -> Option<Title> {
    let line = answer.lines().map(str::trim).find(|line| !line.is_empty())?;
    let plain: String = line.trim_start_matches('#').chars().filter(|c| !matches!(c, '*' | '`')).collect();
    let plain = plain.trim();
    let plain = plain.strip_prefix("Title:").unwrap_or(plain);
    let plain = plain.trim_matches(|c: char| matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’') || c.is_whitespace());
    Title::from_first_line(plain.strip_suffix('.').unwrap_or(plain))
}

#[cfg(test)]
mod tests {
    use protocol::attachment::{Attachment, AttachmentBytes, MediaType};

    use super::*;

    fn tidied(answer: &str) -> Option<String> {
        tidy(answer).map(|title| title.to_string())
    }

    #[test]
    fn a_title_is_tidied_onto_one_line() {
        assert_eq!(tidied("Locate sidebar PR state source").as_deref(), Some("Locate sidebar PR state source"));
        assert_eq!(tidied("\n  \"Fix the login redirect.\"  \nBecause…").as_deref(), Some("Fix the login redirect"));
        assert_eq!(tidied("**Title:** Speed up   the diff").as_deref(), Some("Speed up the diff"));
        assert_eq!(tidied("# Retry setup").as_deref(), Some("Retry setup"));
        assert_eq!(tidied("Close #123 and `Workspace`'s tabs").as_deref(), Some("Close #123 and Workspace's tabs"));
        assert_eq!(tidied(" \n \"\" "), None);
    }

    /// Lines as the CLI prints them for a title, cut down to what's read, with one after the result.
    fn printed(result: &str) -> Vec<u8> {
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"s"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Not this"}]}}"#,
            result,
            r#"{"type":"system","subtype":"session_state_changed","state":"idle"}"#,
        ];
        lines.join("\n").into_bytes()
    }

    #[test]
    fn a_title_comes_only_from_a_success() {
        let ok =
            printed(r#"{"type":"result","subtype":"success","is_error":false,"result":"Read settings from TOML"}"#);
        assert_eq!(title_from_output(&ok).unwrap().as_str(), "Read settings from TOML");
        let failed = br#"{"type":"result","subtype":"error_during_execution","is_error":true}"#;
        assert!(matches!(title_from_output(failed), Err(NoTitle::Reported(kind)) if kind == "error_during_execution"));
        let refused = br#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in"}"#;
        assert!(matches!(title_from_output(refused), Err(NoTitle::Reported(why)) if why == "Not logged in"));
        let empty = br#"{"type":"result","subtype":"success","is_error":false,"result":"  "}"#;
        assert!(matches!(title_from_output(empty), Err(NoTitle::Empty)));
        assert!(matches!(title_from_output(b"not json"), Err(NoTitle::NoResult)));
        assert!(matches!(title_from_output(&printed("")), Err(NoTitle::NoResult)), "nothing but the lines around it");
    }

    /// The CLI says it isn't logged in only on stdout, in its `result` line, and exits with an error.
    #[test]
    fn a_failure_says_why_from_the_result_line_or_else_stderr() {
        use std::os::unix::process::ExitStatusExt as _;
        let failed = |stdout: Vec<u8>, stderr: &str| Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout,
            stderr: stderr.as_bytes().to_vec(),
        };

        let logged_out = printed(r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in"}"#);
        assert_eq!(said(&failed(logged_out, "")), "Not logged in");
        assert_eq!(said(&failed(Vec::new(), " unknown option\n")), "unknown option");
    }

    fn image(media_type: MediaType, bytes: &[u8]) -> NewAttachment {
        let size_bytes = u64::try_from(bytes.len()).unwrap();
        let attachment = Attachment { id: "a".into(), name: "shot".to_string(), media_type, size_bytes };
        NewAttachment { attachment, data: AttachmentBytes::new(bytes.to_vec()) }
    }

    #[test]
    fn the_images_come_before_the_words_and_nothing_else_goes() {
        let (shot, chart) = (image(MediaType::Png, b"\x89PNG"), image(MediaType::Jpeg, b"\xff\xd8\xff"));
        let newer = image(MediaType::Unknown, b"?");

        let line = asked("How do we fix this?", &[shot.clone(), newer, chart.clone()]);

        let content = line["message"]["content"].as_array().unwrap();
        let block = |image: &NewAttachment| image_block(image.attachment.media_type, &image.data);
        assert_eq!(content[..2], [block(&shot), block(&chart)], "a kind from a newer hub is left out");
        assert_eq!(content[2], json!({"type": "text", "text": "How do we fix this?"}));
        assert_eq!(content.len(), 3);
        assert_eq!(line, user_line(content.clone()), "a user line, as a session's prompt is");
        let images_alone = asked(" ", std::slice::from_ref(&shot));
        assert_eq!(images_alone["message"]["content"], json!([block(&shot)]), "no empty words after them");
    }

    /// What keeps the call to writing a title: nothing it could do, no session saved, and none of the user's hooks.
    #[test]
    fn the_call_can_do_nothing_but_answer() {
        let args = title_args();
        let after = |flag: &str| args.iter().position(|arg| arg == flag).map(|i| args[i + 1].as_str());

        assert_eq!(after("--tools"), Some(""), "no tools");
        assert_eq!((after("--model"), after("--effort")), (Some("haiku"), Some("high")), "never the user's own");
        assert_eq!(after("--settings"), Some(r#"{"disableAllHooks":true}"#));
        assert_eq!(after("--setting-sources"), Some("user"), "no project's settings");
        let formats = (after("--input-format"), after("--output-format"));
        assert_eq!(formats, (Some("stream-json"), Some("stream-json")), "lines in, which the CLI answers with lines");
        for flag in ["-p", "--verbose", "--strict-mcp-config", "--no-session-persistence", "--disable-slash-commands"] {
            assert!(args.iter().any(|arg| arg == flag), "{flag}");
        }
    }
}
