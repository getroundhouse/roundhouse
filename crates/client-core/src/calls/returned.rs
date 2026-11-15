//! What came back for a call, as every client reads it, from either form a result is recorded in: typed, as an adapter
//! that types its calls records it, or as the agent's words, as one that doesn't records them.

use std::borrow::Cow;

use protocol::attachment::Attachment;
use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::{CommandOutput, Ended, McpContent};
use protocol::event::{ContentBlock, Denied, FileChange};

use super::{Failure, ResultForm, typed};

/// What came back for a call, read from the block it came in.
#[derive(Clone, Debug)]
pub struct Returned<'a> {
    pub is_error: bool,
    /// The person wouldn't let the tool be used, which isn't a failure, so it's said apart.
    pub denied: Option<&'a Denied>,
    /// What came back, as text: as the agent was told it, or for a typed result, as [`typed::output`] reads it.
    pub text: Cow<'a, str>,
    /// What came back, typed, from an adapter that records it so, which a client draws by its kind.
    pub output: Option<&'a CallOutput>,
    /// What it changed in a file, when its agent said, as one file's change of a typed result does.
    pub change: Option<&'a FileChange>,
    /// The images that came back: what the call is about, such as a read of one, then any beside it.
    pub images: Vec<&'a Attachment>,
    /// What came back is for the model alone, so `text` holds none of it.
    pub encrypted: bool,
    form: ResultForm,
}

impl<'a> Returned<'a> {
    /// What the result `block` says came back, or `None` for any other block.
    pub fn of(block: &'a ContentBlock) -> Option<Self> {
        let form = ResultForm::of(block)?;
        match block {
            ContentBlock::CallResult { output, is_error, denied, images, .. } => Some(Self {
                is_error: *is_error,
                denied: denied.as_ref(),
                text: Cow::Owned(typed::output(output)),
                output: Some(output),
                change: change_of(output),
                images: images_of(output).chain(images).collect(),
                encrypted: matches!(output, CallOutput::Encrypted),
                form,
            }),
            ContentBlock::ToolResult { is_error, text, images, change, encrypted, denied, .. } => Some(Self {
                is_error: *is_error,
                denied: denied.as_ref(),
                text: Cow::Borrowed(text),
                output: None,
                change: change.as_ref(),
                images: images.iter().collect(),
                encrypted: *encrypted,
                form,
            }),
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Call { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => None,
        }
    }

    /// The form it was recorded in, which says how its text is read.
    pub fn form(&self) -> ResultForm {
        self.form
    }

    /// What it says went wrong, if it failed. A call the person denied didn't fail: it never ran, and what came back
    /// says what they said rather than what went wrong.
    pub fn failure(&self) -> Option<Failure<'_>> {
        (self.is_error && self.denied.is_none()).then(|| Failure::read(&self.text, self.form))
    }

    /// Whether the call goes on in the background, as [`in_the_background`] says.
    pub fn in_the_background(&self) -> bool {
        self.output.is_some_and(in_the_background)
    }
}

/// Whether the call `output` came back for goes on in the background, as the task the hub lists, so what came back is
/// only what it had by then: a command, beside what it printed before, or any other call, with nothing else.
pub fn in_the_background(output: &CallOutput) -> bool {
    matches!(
        output,
        CallOutput::Backgrounded { .. } | CallOutput::Command(CommandOutput { ended: Ended::Backgrounded { .. }, .. })
    )
}

/// The images that are what the call is about.
fn images_of(output: &CallOutput) -> impl Iterator<Item = &Attachment> {
    let (image, content) = match output {
        CallOutput::Image { image, .. } => (Some(image), [].as_slice()),
        CallOutput::Mcp { content, .. } => (None, content.as_slice()),
        CallOutput::Command(_)
        | CallOutput::Lines(_)
        | CallOutput::Document { .. }
        | CallOutput::Changed { .. }
        | CallOutput::Found(_)
        | CallOutput::Paths(_)
        | CallOutput::Page { .. }
        | CallOutput::WebResults { .. }
        | CallOutput::Report { .. }
        | CallOutput::Words { .. }
        | CallOutput::Encrypted
        | CallOutput::Backgrounded { .. }
        | CallOutput::Unknown => (None, [].as_slice()),
    };
    let sent = content.iter().filter_map(|content| match content {
        McpContent::Image { image } => Some(image),
        McpContent::Text { .. } | McpContent::Resource { .. } | McpContent::Other { .. } | McpContent::Unknown => None,
    });
    image.into_iter().chain(sent)
}

/// What a typed result changed in a file, when it changed one and its agent said how.
fn change_of(output: &CallOutput) -> Option<&FileChange> {
    let CallOutput::Changed { files } = output else { return None };
    let [file] = files.as_slice() else { return None };
    file.change.as_ref()
}

#[cfg(test)]
mod tests {
    use protocol::capability::calls::CallOutput;
    use protocol::capability::calls::output::{Ended, FileOutcome, McpContent};
    use protocol::event::{CallKind, ContentBlock, FileChange};
    use protocol::fixtures::{self, image};

    use super::Returned;
    use crate::calls::Ending;

    /// A result's form is whether it says what kind of call it answers, whatever that kind is.
    #[test]
    fn a_recorded_result_is_read_by_whether_it_says_its_kind() {
        let before = fixtures::tool_result("c", "Exit code 2\nno such file", true);
        let since = fixtures::of_kind(fixtures::command_failed("c", "no such file", 2), CallKind::Unknown);
        let failure = |block: &ContentBlock| {
            Returned::of(block).and_then(|returned| returned.failure().map(|f| (f.ended, f.output.to_string())))
        };

        assert_eq!(failure(&before), Some((Some(Ending::Exited(2)), "no such file".into())));
        assert_eq!(failure(&since), Some((Some(Ending::Exited(2)), "no such file".into())));
        assert_eq!(failure(&fixtures::tool_result("c", "a.rs", false)), None, "it didn't fail");
        assert_eq!(failure(&fixtures::tool_denied("c", "Not that", Some("Not that"))), None, "it was denied");
        assert!(Returned::of(&fixtures::tool_use("c", "Bash", serde_json::json!({}))).is_none(), "not a result");
    }

    /// A typed command says how it ended beside what it printed, so its failure says that rather than reading its words.
    #[test]
    fn a_typed_command_that_failed_says_how_it_ended() {
        let label = |exit_code, ended| {
            let block = fixtures::typed_result(
                "c",
                fixtures::command_output("Exit code 9\nno such file", exit_code, ended),
                true,
            );
            let returned = Returned::of(&block).unwrap();
            let failure = returned.failure().unwrap();
            (failure.label(), failure.output.to_string())
        };

        assert_eq!(label(Some(2), Ended::Exited), ("Exit 2".into(), "Exit code 9\nno such file".into()));
        assert_eq!(label(None, Ended::TimedOut).0, "Timed out");
        assert_eq!(label(None, Ended::Stopped).0, "Stopped");
        assert_eq!(label(Some(130), Ended::Stopped).0, "Stopped", "that it was stopped says more than its code");
        assert_eq!(label(None, Ended::Exited).0, "Error");
    }

    #[test]
    fn a_typed_call_the_person_denied_did_not_fail() {
        let block = fixtures::typed_denied("c", "The user said no", Some("Not that"));

        let returned = Returned::of(&block).unwrap();

        assert!(returned.failure().is_none());
        assert_eq!(returned.denied.and_then(|denied| denied.said.as_deref()), Some("Not that"));
    }

    /// An image that's what the call is about comes first, then those that came back beside it.
    #[test]
    fn a_typed_result_has_its_own_images_then_those_beside_it() {
        let content = vec![McpContent::Text { text: "two".into() }, McpContent::Image { image: image("sent") }];
        let mcp = fixtures::typed_result("c", CallOutput::Mcp { content, structured: None }, false);
        let block = fixtures::with_images(mcp, vec![image("beside")]);

        let returned = Returned::of(&block).unwrap();

        let ids: Vec<&str> = returned.images.iter().map(|image| image.id.as_str()).collect();
        assert_eq!(ids, ["sent", "beside"]);
        assert_eq!(returned.text, "two");
    }

    #[test]
    fn a_result_for_the_model_alone_is_encrypted_however_it_was_recorded() {
        let encrypted = |block: ContentBlock| Returned::of(&block).unwrap().encrypted;
        let mut recorded = fixtures::tool_result("c", "", false);
        if let ContentBlock::ToolResult { encrypted, .. } = &mut recorded {
            *encrypted = true;
        }

        assert!(encrypted(fixtures::typed_result("c", CallOutput::Encrypted, false)));
        assert!(encrypted(recorded));
        assert!(!encrypted(fixtures::typed_result("c", CallOutput::Words { text: "a".into() }, false)));
        assert!(!encrypted(fixtures::tool_result("c", "a", false)));
    }

    /// A result recorded as the agent's words keeps its images, and what it changed, as its adapter said them.
    #[test]
    fn a_result_recorded_as_words_has_what_its_adapter_said() {
        let mut block = fixtures::with_images(fixtures::tool_result("c", "a screenshot", false), vec![image("shot")]);
        if let ContentBlock::ToolResult { change, .. } = &mut block {
            *change = Some(FileChange::Created);
        }

        let returned = Returned::of(&block).unwrap();

        let ids: Vec<&str> = returned.images.iter().map(|image| image.id.as_str()).collect();
        assert_eq!(
            (ids.as_slice(), returned.change, returned.text.as_ref()),
            (["shot"].as_slice(), Some(&FileChange::Created), "a screenshot")
        );
        assert!(returned.output.is_none());
    }

    /// A change is one file's, as a call that changed one says how.
    #[test]
    fn a_typed_result_that_changed_one_file_says_how() {
        let file = |path: &str| FileOutcome {
            path: path.into(),
            change: Some(FileChange::Created),
            moved_to: None,
            diagnostics: Vec::new(),
        };
        let changed = |files| fixtures::typed_result("c", CallOutput::Changed { files }, false);

        assert_eq!(Returned::of(&changed(vec![file("a.rs")])).unwrap().change, Some(&FileChange::Created));
        assert_eq!(Returned::of(&changed(vec![file("a.rs"), file("b.rs")])).unwrap().change, None);
    }

    #[test]
    fn a_command_that_goes_on_in_the_background_says_so() {
        let ended = Ended::Backgrounded { task: "t".into() };
        let backgrounded = fixtures::typed_result("c", fixtures::command_output("", None, ended), false);
        let exited = fixtures::typed_result("c", fixtures::command_output("", Some(0), Ended::Exited), false);
        let any_other = fixtures::typed_result("c", CallOutput::Backgrounded { task: Some("t".into()) }, false);

        assert!(Returned::of(&backgrounded).unwrap().in_the_background());
        assert!(Returned::of(&any_other).unwrap().in_the_background());
        assert!(!Returned::of(&exited).unwrap().in_the_background());
        assert!(!Returned::of(&fixtures::tool_result("c", "", false)).unwrap().in_the_background());
    }
}
