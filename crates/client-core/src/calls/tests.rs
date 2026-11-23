use protocol::capability::approvals::Asks;
use protocol::capability::calls::call::{
    CellEdit, Command as TypedCommand, CommandPart, Edit, Fetch, FilePatch, FindFiles, GenerateImage, NotebookEdit,
    Patch, PatchChange, Replace, Replaces, Runs, Search as TypedSearch, Shows, StartSubagent, ViewImage, WebSearch,
};
use protocol::capability::calls::output::{Found, FoundLine, Paths, WebResult};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::EventBody;
use protocol::fixtures::{self, approval_request, asking, plan};
use serde_json::{Value, json};

use super::{Looks, Only, ReadFile, Recorded, Search, ToolCall, count, typed};

/// A request that asks `asks`.
fn request(asks: Asks) -> EventBody {
    approval_request("a", asks).into()
}

fn summary(call: &Call) -> Option<String> {
    Recorded::Typed(call).summary()
}

fn search(pattern: &str, path: Option<&str>) -> Call {
    Call::Search(TypedSearch { pattern: pattern.into(), path: path.map(Into::into), files: None, shows: Shows::Lines })
}

fn subagent(description: Option<&str>, prompt: &str) -> Call {
    Call::Subagent(StartSubagent {
        persona: None,
        description: description.map(Into::into),
        prompt: prompt.into(),
        model: None,
        effort: None,
        runs: Runs::Waits,
        resumes: None,
    })
}

#[test]
fn a_call_s_summary_is_the_input_that_matters_most_for_its_kind() {
    let command =
        TypedCommand { description: Some("Run the tests".into()), ..fixtures::typed_command("\ncargo test\nmore") };
    assert_eq!(summary(&Call::Command(command)).as_deref(), Some("cargo test"), "its first line with anything on it");
    let replace = Replace { old: "y".into(), new: "z".into(), replaces: Replaces::One };
    let edit = Call::Edit(Edit { path: "src/a.rs".into(), edits: vec![replace] });
    assert_eq!(summary(&edit).as_deref(), Some("src/a.rs"));
    assert_eq!(summary(&search("fn main", Some("src"))).as_deref(), Some("fn main"), "what it looks for, not where");
    let fetch = Call::Fetch(Fetch { url: "https://docs.rs/serde".into(), asking: Some("What derives?".into()) });
    assert_eq!(summary(&fetch).as_deref(), Some("https://docs.rs/serde"));
    let web =
        Call::WebSearch(WebSearch { query: "rail signals".into(), only_domains: Vec::new(), not_domains: Vec::new() });
    assert_eq!(summary(&web).as_deref(), Some("rail signals"));
    assert_eq!(summary(&Call::Advisor), None, "it takes no input");
}

#[test]
fn a_subagent_s_summary_is_what_it_s_for_or_else_what_it_was_asked() {
    assert_eq!(summary(&subagent(Some("Find the tests"), "Look for\nthe tests")).as_deref(), Some("Find the tests"));
    assert_eq!(summary(&subagent(None, "Look for\nthe tests")).as_deref(), Some("Look for"));
    assert_eq!(summary(&subagent(None, "")), None);
}

#[test]
fn any_other_tool_s_summary_is_its_usual_input_or_else_its_first() {
    let other = |tool: &str, input: Value| Call::Other { tool: tool.into(), input };
    let mcp = Call::Mcp { server: "github".into(), tool: "create_issue".into(), input: json!({ "title": "Signals" }) };
    assert_eq!(summary(&mcp).as_deref(), Some("title: \"Signals\""));
    assert_eq!(summary(&other("ToolSearch", json!({ "query": "select:Read\nmore" }))).as_deref(), Some("select:Read"));
    let described = json!({ "description": " Tidy up ", "todos": [] });
    assert_eq!(summary(&other("TodoList", described)).as_deref(), Some("Tidy up"), "what it's for says more");
    assert_eq!(summary(&other("TodoList", json!({ "todos": [] }))).as_deref(), Some("todos: []"));
    assert_eq!(summary(&other("Ping", json!({}))), None);
}

#[test]
fn a_count_says_one_or_many() {
    assert_eq!(count(1, "line", "lines"), "1 line");
    assert_eq!(count(0, "line", "lines"), "0 lines");
    assert_eq!(count(12, "match", "matches"), "12 matches");
    assert_eq!(count(1_204, "line", "lines"), "1,204 lines", "whole");
}

/// So a client needs no agent's name for a tool: the call is what its adapter typed.
#[test]
fn a_call_is_read_by_what_its_adapter_typed() {
    let looks = Call::FindFiles(FindFiles { pattern: "*.rs".into(), path: None });
    assert!(
        matches!(ToolCall::read(Recorded::Typed(&looks)), ToolCall::Search(search) if search.looks == Looks::ForNames)
    );
    assert!(matches!(ToolCall::read(Recorded::Typed(&Call::Advisor)), ToolCall::Advisor));
    assert!(matches!(ToolCall::read(Recorded::Typed(&subagent(None, "Look"))), ToolCall::Agent(_)));
    let named_like_a_command = Call::Other { tool: "Bash".into(), input: json!({ "command": "ls" }) };
    assert!(matches!(ToolCall::read(Recorded::Typed(&named_like_a_command)), ToolCall::Other(_)), "not by its name");
}

#[test]
fn what_a_call_does_is_its_kind() {
    assert!(Recorded::Typed(&subagent(None, "Look")).hands_off());
    assert!(!Recorded::Typed(&Call::Command(fixtures::typed_command("ls"))).hands_off());
    assert!(Recorded::Typed(&Call::Questions).asks_the_person() && Recorded::Typed(&Call::Plan).asks_the_person());
    let named = Call::Other { tool: "ExitPlanMode".into(), input: Value::Null };
    assert!(!Recorded::Typed(&named).asks_the_person(), "not by its name");
}

/// A call is made only from its block, or the request to make it.
#[test]
fn a_call_is_read_from_its_block_and_from_a_request_to_make_it() {
    let block = fixtures::typed_call("c", Call::Command(fixtures::typed_command("ls")));
    assert!(matches!(ToolCall::read(Recorded::of(&block).unwrap()), ToolCall::Command(_)));
    let result = fixtures::typed_result("c", CallOutput::Words { text: "a.rs".into() }, false);
    assert!(Recorded::of(&result).is_none(), "a result is no call");

    let said = request(Asks::Call { call: Call::Command(fixtures::typed_command("ls -la")) });
    assert!(matches!(ToolCall::read(Recorded::asked(&said).unwrap()), ToolCall::Command(_)));
    assert!(Recorded::asked(&EventBody::Unknown).is_none());
}

/// Questions, a plan, and what this build can't read are no call, whatever made them.
#[test]
fn a_request_that_asks_anything_but_a_call_makes_none() {
    for asks in [asking(Vec::new()), plan(""), Asks::Unknown] {
        assert!(Recorded::asked(&request(asks.clone())).is_none(), "{asks:?}");
    }
}

/// A typed command, which its agent read as doing what `parts` says.
fn command_doing(command: &str, parts: Vec<CommandPart>) -> Call {
    Call::Command(TypedCommand { parts, ..fixtures::typed_command(command) })
}

/// What a typed command does and nothing else, as its agent read it, or `None`.
fn only(call: &Call) -> Option<Only<'_>> {
    let ToolCall::Command(command) = ToolCall::read(Recorded::Typed(call)) else { panic!("still a command") };
    command.only
}

/// A command its agent says does nothing but read one file says which, so a client can draw it as reading that file.
/// One that reads more, or does anything else, or whose agent said nothing of what it does, is only a command.
#[test]
fn a_typed_command_that_only_reads_one_file_says_which() {
    let read = |path: &str| CommandPart::Read { path: path.into() };

    assert_eq!(only(&command_doing("sed -n 1,40p a.rs", vec![read("a.rs")])), Some(Only::Reads("a.rs")));
    assert_eq!(only(&command_doing("cat a.rs a.rs", vec![read("a.rs"), read("a.rs")])), Some(Only::Reads("a.rs")));
    assert_eq!(only(&command_doing("cat a.rs b.rs", vec![read("a.rs"), read("b.rs")])), None, "two files");
    let listed = vec![read("a.rs"), CommandPart::List { path: None }];
    assert_eq!(only(&command_doing("cat a.rs; ls", listed)), None, "more than reading");
    assert_eq!(only(&command_doing("cat a.rs", Vec::new())), None, "its agent didn't say");
}

/// A command its agent says does nothing but search what's in files for one pattern says the search, so a client can
/// draw it as one. One that searches for something its agent couldn't say, or searches twice, or does more, is only a
/// command.
#[test]
fn a_typed_command_that_only_searches_says_how() {
    let search = |pattern: Option<&str>, path: Option<&str>| CommandPart::Search {
        pattern: pattern.map(Into::into),
        path: path.map(Into::into),
    };
    let found = Search { pattern: "fn main", path: Some("src"), looks: Looks::InFiles };

    let searched = command_doing("rg -n 'fn main' src", vec![search(Some("fn main"), Some("src"))]);
    assert_eq!(only(&searched), Some(Only::Searches(found)));
    let unsaid = command_doing("rg -f patterns.txt", vec![search(None, None)]);
    assert_eq!(only(&unsaid), None, "a pattern its agent didn't say");
    let twice = command_doing("rg a; rg b", vec![search(Some("a"), None), search(Some("b"), None)]);
    assert_eq!(only(&twice), None, "two searches");
    let read = CommandPart::Read { path: "a.rs".into() };
    assert_eq!(only(&command_doing("rg a; cat a.rs", vec![search(Some("a"), None), read])), None, "and a read");
}

/// A typed search, a find, a fetch and a web search are each read as the kind a client draws, and named in words.
#[test]
fn a_typed_search_or_call_of_the_web_is_read_as_its_kind_and_named_in_words() {
    let searched = Call::Search(TypedSearch {
        pattern: "fn main".into(),
        path: Some("src".into()),
        files: None,
        shows: Shows::Lines,
    });
    let found = Call::FindFiles(FindFiles { pattern: "**/*.rs".into(), path: None });
    let fetched = Call::Fetch(Fetch { url: "https://example.com/a".into(), asking: Some("What's new?".into()) });
    let web = Call::WebSearch(WebSearch { query: "signals".into(), only_domains: Vec::new(), not_domains: Vec::new() });
    let read = |call| ToolCall::read(Recorded::Typed(call));

    assert!(
        matches!(read(&searched), ToolCall::Search(s) if s == Search { pattern: "fn main", path: Some("src"), looks: Looks::InFiles })
    );
    assert!(matches!(read(&found), ToolCall::Search(s) if s.looks == Looks::ForNames && s.path.is_none()));
    assert!(
        matches!(read(&fetched), ToolCall::Fetch(f) if f.url == "https://example.com/a" && f.prompt == Some("What's new?"))
    );
    assert!(matches!(read(&web), ToolCall::WebSearch(w) if w.query == "signals"));
    let said = |call| (typed::title(call), Recorded::Typed(call).summary().unwrap_or_default());
    assert_eq!(said(&searched), ("Search".into(), "fn main".into()));
    assert_eq!(said(&found), ("Find files".into(), "**/*.rs".into()));
    assert_eq!(said(&fetched), ("Fetch".into(), "https://example.com/a".into()));
    assert_eq!(said(&web), ("Search the web".into(), "signals".into()));
}

/// Each typed call of a file is read as the kind a client draws, and named in words. A look at an image is a read of
/// it, and making one is a kind of its own, which says what it was asked for, unless that's not been said yet, as
/// Codex says it only once the image is made.
#[test]
fn a_typed_call_of_a_file_is_read_as_its_kind_and_named_in_words() {
    let looked_at = Call::ViewImage(ViewImage { path: "/tmp/shot.png".into() });
    let made =
        Call::GenerateImage(GenerateImage { prompt: "A signal at caution\nin the rain".into(), from: Vec::new() });
    let making = Call::GenerateImage(GenerateImage { prompt: String::new(), from: Vec::new() });
    let notebook = Call::NotebookEdit(NotebookEdit {
        path: "plots.ipynb".into(),
        cell: None,
        source: "plot(x)".into(),
        does: CellEdit::InsertAfter,
    });
    let patch = Call::Patch(Patch {
        files: vec![
            FilePatch { path: "src/a.rs".into(), change: PatchChange::Delete },
            FilePatch { path: "src/b.rs".into(), change: PatchChange::Add { content: "b".into() } },
        ],
    });
    let read = |call| ToolCall::read(Recorded::Typed(call));

    assert!(matches!(read(&notebook), ToolCall::NotebookEdit(_)));
    assert!(matches!(read(&patch), ToolCall::Patch(_)));
    assert!(matches!(read(&looked_at), ToolCall::Read(ReadFile { path: "/tmp/shot.png", .. })));
    assert!(matches!(read(&made), ToolCall::MakeImage(_)));
    let said = |call| (typed::title(call), Recorded::Typed(call).summary().unwrap_or_default());
    assert_eq!(said(&notebook), ("Add a cell".into(), "plots.ipynb".into()));
    assert_eq!(said(&patch), ("Edit".into(), "src/a.rs, src/b.rs".into()));
    assert_eq!(said(&looked_at), ("View an image".into(), "/tmp/shot.png".into()));
    assert_eq!(said(&made), ("Make an image".into(), "A signal at caution".into()));
    assert_eq!(Recorded::Typed(&making).summary(), None, "nothing said yet");
}

/// What a typed search found reads as text, as the CLI prints it: its matches as ripgrep prints them, its files one to
/// a line, or how many it counted, and a web search's pages, each its title and address, then what was made of them.
#[test]
fn what_a_typed_search_found_reads_as_text() {
    let line = FoundLine { path: "src/a.rs".into(), line: 3, text: "fn a() {".into() };
    let lines = CallOutput::Found(Found::Lines { lines: vec![line], truncated: false });
    let files = Paths { paths: vec!["src/a.rs".into(), "src/b.rs".into()], truncated: false };
    let page = WebResult { title: "Railway signal".into(), url: "https://example.org/signal".into(), snippet: None };
    let pages = CallOutput::WebResults { results: vec![page], summary: Some("A signal shows stop.".into()) };

    assert_eq!(typed::output(&lines), "src/a.rs:3:fn a() {");
    assert_eq!(typed::output(&CallOutput::Found(Found::Files(files))), "src/a.rs\nsrc/b.rs");
    assert_eq!(typed::output(&CallOutput::Found(Found::Count { matches: 3, files: 2 })), "3 matches in 2 files");
    assert_eq!(typed::output(&pages), "Railway signal (https://example.org/signal)\n\nA signal shows stop.");
    let words = CallOutput::WebResults { results: Vec::new(), summary: Some("Nothing matched.".into()) };
    assert_eq!(typed::output(&words), "Nothing matched.", "no pages, then what was made of none");
}

/// An image reads as what it was made from, as the CLI prints it beside the image, and one the agent only looked at
/// reads as nothing.
#[test]
fn an_image_reads_as_what_it_was_made_from() {
    let image = |prompt: Option<&str>| CallOutput::Image {
        image: protocol::fixtures::image("made"),
        prompt: prompt.map(Into::into),
    };

    assert_eq!(typed::output(&image(Some("A signal at caution"))), "A signal at caution");
    assert_eq!(typed::output(&image(None)), "");
}
