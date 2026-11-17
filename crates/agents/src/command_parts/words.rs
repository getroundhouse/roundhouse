//! A shell script's commands and their words, as far as reading them takes no shell. This reads plain words, quotes,
//! and the pipes and lists that join commands, `|`, `&&`, `||`, `;` and new lines, and gives up on anything that would
//! need the shell to say what a word becomes or what a command reads or writes: a redirect, a `$` outside single
//! quotes, as an expansion, a substitution or a `$'…'` quote has, a glob or a brace expansion, which can make one word
//! many, a word zsh would take as a command's path, a comment, a subshell or a group, a command run in the background,
//! or a quote left open.

/// One command's words, with their quotes taken off.
pub(super) type Words = Vec<String>;

/// The commands of one pipeline, each piped into the next.
pub(super) type Pipeline = Vec<Words>;

/// The pipelines `script` runs one after another, each as its commands' words. `None` for a script that does anything
/// more than run them.
pub(super) fn pipelines(script: &str) -> Option<Vec<Pipeline>> {
    let mut read = Reading::default();
    let mut chars = script.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => read.end_word(),
            '\n' | ';' => read.end_pipeline()?,
            '|' => {
                if chars.next_if_eq(&'|').is_some() {
                    read.end_pipeline()?;
                } else if chars.next_if_eq(&'&').is_some() {
                    // `|&` pipes what it says of errors too, which reads the same, but is rare enough to leave.
                    return None;
                } else {
                    read.end_command()?;
                }
            }
            '&' => {
                chars.next_if_eq(&'&')?;
                read.end_pipeline()?;
            }
            '<' | '>' | '(' | ')' | '`' | '$' | '#' | '*' | '?' | '[' | '{' => return None,
            // zsh takes a word that starts with `=` as the path of the command it names.
            '=' if read.word.is_none() => return None,
            '\\' => match chars.next() {
                Some('\n') => {}
                Some(escaped) => read.push(escaped),
                None => return None,
            },
            '\'' => {
                read.start_word();
                loop {
                    match chars.next()? {
                        '\'' => break,
                        quoted => read.push(quoted),
                    }
                }
            }
            '"' => {
                read.start_word();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '`' | '$' => return None,
                        '\\' => match chars.next()? {
                            escaped @ ('"' | '\\' | '$' | '`') => read.push(escaped),
                            '\n' => {}
                            other => {
                                read.push('\\');
                                read.push(other);
                            }
                        },
                        quoted => read.push(quoted),
                    }
                }
            }
            other => read.push(other),
        }
    }
    read.end_pipeline()?;
    Some(read.pipelines)
}

/// What's been read so far.
#[derive(Default)]
struct Reading {
    pipelines: Vec<Pipeline>,
    pipeline: Pipeline,
    command: Words,
    /// The word being read, once a character or a quote has started it, since `''` is a word too.
    word: Option<String>,
}

impl Reading {
    fn start_word(&mut self) {
        self.word.get_or_insert_with(String::new);
    }

    fn push(&mut self, c: char) {
        self.word.get_or_insert_with(String::new).push(c);
    }

    fn end_word(&mut self) {
        if let Some(word) = self.word.take() {
            self.command.push(word);
        }
    }

    /// Ends a command that a `|` pipes into the next. `None` where there's no command to pipe.
    fn end_command(&mut self) -> Option<()> {
        self.end_word();
        if self.command.is_empty() {
            return None;
        }
        self.pipeline.push(std::mem::take(&mut self.command));
        Some(())
    }

    /// Ends the pipeline being read. One with no commands, as after a `;` at the end, is none. `None` where a `|` pipes
    /// into nothing.
    fn end_pipeline(&mut self) -> Option<()> {
        self.end_word();
        if self.command.is_empty() {
            if !self.pipeline.is_empty() {
                return None;
            }
        } else {
            self.pipeline.push(std::mem::take(&mut self.command));
        }
        if !self.pipeline.is_empty() {
            self.pipelines.push(std::mem::take(&mut self.pipeline));
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::pipelines;

    /// Pipelines, each as its commands' words.
    fn words(pipelines: &[&[&[&str]]]) -> Vec<Vec<Vec<String>>> {
        pipelines
            .iter()
            .map(|pipeline| {
                pipeline.iter().map(|command| command.iter().map(|word| (*word).to_owned()).collect()).collect()
            })
            .collect()
    }

    #[test]
    fn a_script_is_its_commands_words_with_their_quotes_taken_off() {
        assert_eq!(pipelines("rg -n 'fn main' src"), Some(words(&[&[&["rg", "-n", "fn main", "src"]]])));
        let quoted = r#"grep "a \"b\"" c\ d.rs '' \*.rs"#;
        assert_eq!(pipelines(quoted), Some(words(&[&[&["grep", r#"a "b""#, "c d.rs", "", "*.rs"]]])));
        assert_eq!(
            pipelines("rg 'a|b;c' src"),
            Some(words(&[&[&["rg", "a|b;c", "src"]]])),
            "joins in quotes are words"
        );
        assert_eq!(pipelines("rg -g '*.rs' \"{a,b}\""), Some(words(&[&[&["rg", "-g", "*.rs", "{a,b}"]]])));
        assert_eq!(pipelines("FOO=bar cat"), Some(words(&[&[&["FOO=bar", "cat"]]])), "a word with = in it");
        let wrapped = "/bin/zsh -lc 'cat '\"'\"'a b'\"'\"''";
        assert_eq!(pipelines(wrapped), Some(words(&[&[&["/bin/zsh", "-lc", "cat 'a b'"]]])));
    }

    #[test]
    fn a_pipe_joins_commands_and_a_list_joins_pipelines() {
        let piped = Some(words(&[&[&["cat", "a.rs"], &["xargs", "rm"]]]));
        assert_eq!(pipelines("cat a.rs | xargs rm"), piped);
        assert_eq!(pipelines("cat a.rs|xargs rm"), piped);
        let listed = Some(words(&[&[&["cat", "a.rs"]], &[&["xargs", "rm"]]]));
        for script in ["cat a.rs && xargs rm", "cat a.rs || xargs rm", "cat a.rs; xargs rm;", "cat a.rs\n\nxargs rm"] {
            assert_eq!(pipelines(script), listed, "{script:?}");
        }
        assert_eq!(pipelines("cat a.rs \\\n  b.rs"), Some(words(&[&[&["cat", "a.rs", "b.rs"]]])), "a line carried on");
    }

    #[test]
    fn a_script_that_does_more_than_run_commands_reads_as_none() {
        for script in [
            "cat a.rs > b.rs",
            "cat < a.rs",
            "cat $(ls)",
            "cat \"$(ls)\"",
            "cat $HOME/a.rs",
            "cat \"$HOME/a.rs\"",
            "cat $'\\'' a.rs; rm -rf build; #'",
            "cat a.rs #'\nrm -rf build\n#'",
            "cat `ls`",
            "(cat a.rs)",
            "{ cat a.rs; }",
            "cat *.rs",
            "cat a?.rs",
            "cat [ab].rs",
            "cat {a,b}.rs",
            "cat =ls",
            "cat a.rs &",
            "cat a.rs |& tee b.rs",
            "cat a.rs |",
            "| cat a.rs",
            "cat a.rs | ; rm b.rs",
            "cat 'a.rs",
            "cat \"a.rs",
            "cat a.rs\\",
        ] {
            assert_eq!(pipelines(script), None, "{script:?}");
        }
    }
}
