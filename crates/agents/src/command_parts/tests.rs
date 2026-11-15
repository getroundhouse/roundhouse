//! What a command's words say it does, as Codex runs them: inside `/bin/zsh -c` and the quotes it gives the script.

use protocol::capability::calls::call::CommandPart;

use super::of;

/// What `script` does, run as Codex runs it.
fn parts(script: &str) -> Vec<CommandPart> {
    of(&format!("/bin/zsh -c '{}'", script.replace('\'', "'\"'\"'")))
}

fn read(path: &str) -> Vec<CommandPart> {
    vec![CommandPart::Read { path: path.into() }]
}

fn listed(path: Option<&str>) -> Vec<CommandPart> {
    vec![CommandPart::List { path: path.map(Into::into) }]
}

fn found(pattern: Option<&str>, path: Option<&str>) -> Vec<CommandPart> {
    vec![CommandPart::Search { pattern: pattern.map(Into::into), path: path.map(Into::into) }]
}

fn other() -> Vec<CommandPart> {
    vec![CommandPart::Other]
}

/// One program that does nothing but print one file, list files in one place or search what's in files for one
/// pattern, alone or cut short by `sed -n`, says so, with the file, the place and the pattern as it names them.
#[test]
fn one_program_that_only_reads_lists_or_searches_says_what() {
    for script in
        ["cat a.rs", "sed -n '1,40p' a.rs", "nl -ba a.rs | sed -n '1,40p'", "head -n 40 a.rs", "nl -n ln a.rs"]
    {
        assert_eq!(parts(script), read("a.rs"), "{script}");
    }
    assert_eq!(parts("tail -20 ./a.rs"), read("./a.rs"));
    assert_eq!(parts("cat 'a b.rs'"), read("a b.rs"));
    assert_eq!(of("/bin/zsh -lc \"sed -n '1,40p' a.rs\""), read("a.rs"), "a script in double quotes");
    assert_eq!(of("cat a.rs"), read("a.rs"), "a script run without a shell around it");
    for script in [
        "rg -n 'fn main' src",
        "rg -n -g '*.rs' 'fn main' src",
        "rg -C3 -- 'fn main' src",
        "grep -rn 'fn main' src",
        "rg -e 'fn main' src",
        "rg 'fn main' src | sed -n 1,20p",
    ] {
        assert_eq!(parts(script), found(Some("fn main"), Some("src")), "{script}");
    }
    assert_eq!(parts("rg 'a|b'"), found(Some("a|b"), None), "a pipe in a quoted pattern");
    assert_eq!(parts("git grep -n 'fn main'"), found(Some("fn main"), None));
    assert_eq!(parts("git grep -n 'fn main' -- src"), found(Some("fn main"), Some("src")));
    assert_eq!(parts("git grep -e 'fn main' -- src"), found(Some("fn main"), Some("src")));
    assert_eq!(parts("rg -e x y"), found(Some("x"), Some("y")));
    assert_eq!(parts("rg -f patterns.txt src"), found(None, Some("src")), "a pattern read from a file");
    assert_eq!(parts("ls src"), listed(Some("src")));
    assert_eq!(parts("ls -la"), listed(None));
    assert_eq!(parts("rg --files -g '*.rs' src"), listed(Some("src")));
    assert_eq!(parts("git ls-files"), listed(None));
}

/// A command of more than one program, other than one cut short by `sed -n`, does more than any one of them says, as
/// does one the reader can't read whole.
#[test]
fn a_command_of_more_than_one_program_does_more() {
    for script in [
        "cat a.rs | xargs rm",
        "cat a.rs && rm b.rs",
        "cat a.rs; rm b.rs",
        "cat a.rs || rm b.rs",
        "cat a.rs\nrm b.rs",
        "cd src && cat a.rs",
        "cat a.rs | head",
        "cat a.rs; sed -n 1p",
        "nl -ba a.rs | sed -n '1,40p' | xargs rm",
        "sed -n 1,5p a.rs | tee b.rs",
        "cat a.rs | sed -n 'w b.rs'",
        "cat a.rs > b.rs",
        "rg -l foo | xargs rm",
        "cat $HOME/a.rs",
        "cat *.rs",
        "cat {a,b}.rs",
        "head -n 5 src/*.rs",
        "rg foo src/*.rs",
    ] {
        assert_eq!(parts(script), other(), "{script:?}");
    }
    assert_eq!(of("/bin/zsh -c 'cat a.rs' b.rs"), other(), "a shell given more than its script");
    assert_eq!(of("fish -c 'cat a.rs'"), other(), "a shell whose scripts this doesn't read");
}

/// A program that can do more than print a file, list files or search them, or is given what can make it do more, or
/// do it to more than one part names, does more.
#[test]
fn a_program_that_can_do_more_does_more() {
    for script in [
        "awk 'BEGIN{system(\"rm -rf build\")}' a.rs",
        "sed -n -e 1,5p -e 'w out.txt' a.rs",
        "sed -n 1,5p -i a.rs",
        "sed -n 'w b.rs' 1p",
        "cat a.rs b.rs",
        "head -n 5 a.rs b.rs",
        "tail -n 5 a.rs b.rs",
        "nl a.rs b.rs",
        "cat -",
        "cat",
        "./cat a.rs",
        "/usr/bin/cat a.rs",
        "less a.rs",
    ] {
        assert_eq!(parts(script), other(), "{script}");
    }
    for script in [
        "find . -name '*.orig' -delete",
        "fd orig",
        "rg --pre rm x .",
        "rg --hostname-bin=rm x",
        "rg -z x",
        "rg -5 x",
        "rg x a b",
        "rg -e x -e y",
        "rg -e x -f patterns.txt",
        "rg",
        "git grep -Orm x",
        "git grep --open x",
        "git grep --textconv x",
        "git grep x HEAD",
        "git grep -e x src",
        "git -C src grep x",
        "grep x -",
    ] {
        assert_eq!(parts(script), other(), "{script}");
    }
    for script in [
        "find . -type f -delete",
        "fd -x /bin/rm",
        "python3 -c 'import os; os.remove(\"a\")'",
        "ls a b",
        "rg --files --pre rm",
        "rg --files -e x",
        "rg --files a b",
    ] {
        assert_eq!(parts(script), other(), "{script}");
    }
}
