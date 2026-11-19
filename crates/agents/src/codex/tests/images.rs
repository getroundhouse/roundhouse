//! Codex's looks at an image and the images it makes, which it names only by their files, as its adapter records them.

use protocol::attachment::{MediaType, NewAttachment};
use protocol::capability::calls::call::{GenerateImage, ViewImage};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ContentBlock, EventBody};
use serde_json::{Value, json};

use super::script::{item, play, turn};
use crate::AgentOutput;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

/// Codex's items, `started` then `completed`, in a turn of their own, played to the adapter.
async fn played(started: Value, completed: Value) -> Vec<AgentOutput> {
    let actions = vec![
        turn("thread", "root-turn", "inProgress"),
        item("thread", "root-turn", started, false),
        item("thread", "root-turn", completed, true),
        turn("thread", "root-turn", "completed"),
    ];
    play(actions).await
}

/// What came back from a call, whether it failed, and the images the event that says it names for the hub to keep.
#[derive(Debug)]
struct Settled<'a> {
    output: &'a CallOutput,
    failed: bool,
    kept: &'a [NewAttachment],
}

/// Each call the adapter recorded, and what came back from each.
fn calls(output: &[AgentOutput]) -> (Vec<&Call>, Vec<Settled<'_>>) {
    let (mut calls, mut results) = (Vec::new(), Vec::new());
    for out in output {
        let AgentOutput::Event { body, attachments, .. } = out else { continue };
        if let EventBody::Assistant { blocks, .. } = body {
            calls.extend(blocks.iter().filter_map(|block| {
                let ContentBlock::Call { call, .. } = block else { return None };
                Some(call)
            }));
        }
        if let EventBody::AgentUser { blocks, .. } = body {
            results.extend(blocks.iter().filter_map(|block| {
                let ContentBlock::CallResult { output, is_error, .. } = block else { return None };
                Some(Settled { output, failed: *is_error, kept: attachments.as_slice() })
            }));
        }
    }
    (calls, results)
}

/// The image a result is, and that it's the one its event has the hub keep, with the bytes the file held.
fn image_of<'a>(settled: &Settled<'a>) -> Option<&'a str> {
    let CallOutput::Image { image, prompt } = settled.output else { panic!("not an image: {settled:?}") };
    let [kept] = settled.kept else { panic!("{} images kept", settled.kept.len()) };
    assert!(!settled.failed, "it didn't fail");
    assert_eq!(&kept.attachment, image, "what's kept is the image the result names");
    assert_eq!((kept.data.as_slice(), image.media_type), (PNG, MediaType::Png));
    prompt.as_deref()
}

/// A look at an image is a typed look at its file, and what came back is the image, which the hub keeps, so the chat
/// can draw it after the file is gone.
#[tokio::test]
async fn a_look_at_an_image_keeps_the_image() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.png").to_string_lossy().into_owned();
    std::fs::write(&path, PNG).unwrap();
    let looked = json!({"type":"imageView","id":"look-1","path":path});

    let output = played(looked.clone(), looked).await;

    let (calls, results) = calls(&output);
    assert_eq!(calls, [&Call::ViewImage(ViewImage { path: path.clone() })]);
    let [looked] = &results[..] else { panic!("{results:?}") };
    assert_eq!(image_of(looked), None, "a look says no prompt");
    let CallOutput::Image { image, .. } = looked.output else { unreachable!() };
    assert_eq!(image.name, "shot.png", "named for its file");
}

/// An image Codex made starts with nothing said of what it was asked for, and comes back as the image, which the hub
/// keeps, with the prompt it was made from.
#[tokio::test]
async fn an_image_codex_made_is_kept_with_what_it_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("made.png").to_string_lossy().into_owned();
    std::fs::write(&saved, PNG).unwrap();
    let making = json!({"type":"imageGeneration","id":"make-1","status":"in_progress","revisedPrompt":null});
    let made = json!({"type":"imageGeneration","id":"make-1","status":"completed",
        "revisedPrompt":"A railway signal at caution","savedPath":saved,
        "inputImages":[{"path":"/w/signal.png","sha256":"ab"}]});

    let output = played(making, made).await;

    let (calls, results) = calls(&output);
    assert_eq!(calls, [&Call::GenerateImage(GenerateImage { prompt: String::new(), from: Vec::new() })]);
    let [made] = &results[..] else { panic!("{results:?}") };
    assert_eq!(image_of(made), Some("A railway signal at caution"));
}

/// An image that can't be kept says why, and keeps nothing, and one Codex couldn't make fails, saying why when Codex
/// says it to more than the model.
#[tokio::test]
async fn an_image_not_kept_or_not_made_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("notes.png").to_string_lossy().into_owned();
    std::fs::write(&text, "not an image").unwrap();
    let missing = dir.path().join("gone.png").to_string_lossy().into_owned();
    let big = dir.path().join("big.png").to_string_lossy().into_owned();
    // Sparse, so it takes no time to make.
    std::fs::File::create(&big).unwrap().set_len(32 * 1024 * 1024 + 1).unwrap();
    let look = |id: &str, path: &str| json!({"type":"imageView","id":id,"path":path});
    let failed = |id: &str, failure: Value| {
        json!({"type":"imageGeneration","id":id,"status":"failed","revisedPrompt":"A signal",
            "failure":failure})
    };
    let making = |id: &str| json!({"type":"imageGeneration","id":id,"status":"in_progress"});
    let limit = json!({"type":"usageLimitExceeded","limitId":"images","resetsAt":null});
    let mut actions = vec![turn("thread", "root-turn", "inProgress")];
    for (started, completed) in [
        (look("text", &text), look("text", &text)),
        (look("gone", &missing), look("gone", &missing)),
        (look("relative", "shot.png"), look("relative", "shot.png")),
        (look("big", &big), look("big", &big)),
        (making("limited"), failed("limited", limit)),
        (making("failed"), failed("failed", Value::Null)),
    ] {
        actions.push(item("thread", "root-turn", started, false));
        actions.push(item("thread", "root-turn", completed, true));
    }
    actions.push(turn("thread", "root-turn", "completed"));

    let output = play(actions).await;

    let (_, results) = calls(&output);
    let said: Vec<(&str, bool)> = results
        .iter()
        .map(|settled| {
            assert!(settled.kept.is_empty(), "nothing kept");
            let CallOutput::Words { text } = settled.output else { panic!("not words: {settled:?}") };
            (text.as_str(), settled.failed)
        })
        .collect();
    let [not_image, gone, relative, too_big, limited, failed] = said[..] else { panic!("{said:?}") };
    assert_eq!(not_image, (format!("The image wasn't kept: {text} isn't an image the model can see").as_str(), false));
    assert!(gone.0.starts_with(&format!("The image wasn't kept: couldn't read {missing}: ")), "{gone:?}");
    assert_eq!(relative, ("The image wasn't kept: shot.png isn't a full path", false));
    assert_eq!(too_big, (format!("The image wasn't kept: {big} is bigger than 32 MB").as_str(), false));
    assert_eq!(limited, ("Codex's limit on making images was reached", true));
    assert_eq!(failed, ("", true), "Codex tells only the model why");
}
