//! The flags each program may be given and still do nothing more, and what a program's words give it beside them.

/// The flags a program may be given and still do nothing more, and which of them take a value, so what it's given
/// beside them can be counted.
pub(super) struct Flags {
    /// Those it may be given, such as `-n` and `--line-number`. `None` for a program none of whose flags does more.
    allowed: Option<&'static [&'static str]>,
    /// Those that take a value: the next word, or what's joined on, as in `-C3` or `--context=3`.
    valued: &'static [&'static str],
}

/// Any flag, none of which takes a value, as `cat`'s don't.
pub(super) const ANY: Flags = Flags { allowed: None, valued: &[] };

pub(super) const HEAD: Flags = Flags { allowed: None, valued: &["-n", "-c", "--lines", "--bytes"] };

pub(super) const TAIL: Flags = Flags {
    allowed: None,
    valued: &["-n", "-c", "-b", "-s", "--lines", "--bytes", "--sleep-interval", "--pid", "--max-unchanged-stats"],
};

pub(super) const NL: Flags = Flags {
    allowed: None,
    valued: &[
        "-b",
        "-d",
        "-f",
        "-h",
        "-i",
        "-l",
        "-n",
        "-s",
        "-v",
        "-w",
        "--body-numbering",
        "--section-delimiter",
        "--footer-numbering",
        "--header-numbering",
        "--line-increment",
        "--join-blank-lines",
        "--number-format",
        "--number-separator",
        "--starting-line-number",
        "--number-width",
    ],
};

pub(super) const LS: Flags = Flags {
    allowed: None,
    valued: &[
        "-I",
        "-T",
        "-w",
        "--ignore",
        "--hide",
        "--tabsize",
        "--width",
        "--format",
        "--sort",
        "--time",
        "--time-style",
        "--block-size",
        "--quoting-style",
        "--indicator-style",
    ],
};

pub(super) const GIT_LS_FILES: Flags = Flags {
    allowed: None,
    valued: &["-x", "-X", "--exclude", "--exclude-from", "--exclude-per-directory", "--format", "--with-tree"],
};

pub(super) const GREP: Flags = Flags {
    allowed: None,
    valued: &[
        "-e",
        "--regexp",
        "-f",
        "--file",
        "-A",
        "-B",
        "-C",
        "-m",
        "-d",
        "-D",
        "--after-context",
        "--before-context",
        "--context",
        "--max-count",
        "--devices",
        "--directories",
        "--binary-files",
        "--label",
        "--group-separator",
        "--include",
        "--exclude",
        "--exclude-dir",
        "--exclude-from",
    ],
};

/// `rg`'s flags that only change what it searches or how it prints what it found, from its help at 15.2.0. Others run
/// another program, as `--pre`, `--hostname-bin` and `-z` do.
pub(super) const RG: Flags = Flags {
    allowed: Some(&[
        "-0",
        "-A",
        "-B",
        "-C",
        "-F",
        "-H",
        "-I",
        "-L",
        "-M",
        "-N",
        "-P",
        "-S",
        "-T",
        "-U",
        "-a",
        "-c",
        "-e",
        "-f",
        "-g",
        "-i",
        "-j",
        "-l",
        "-m",
        "-n",
        "-o",
        "-q",
        "-r",
        "-s",
        "-t",
        "-u",
        "-v",
        "-w",
        "-x",
        "--after-context",
        "--before-context",
        "--byte-offset",
        "--case-sensitive",
        "--color",
        "--column",
        "--context",
        "--count",
        "--count-matches",
        "--file",
        "--files",
        "--files-with-matches",
        "--files-without-match",
        "--fixed-strings",
        "--follow",
        "--glob",
        "--heading",
        "--hidden",
        "--iglob",
        "--ignore-case",
        "--invert-match",
        "--json",
        "--line-number",
        "--line-regexp",
        "--max-columns",
        "--max-count",
        "--max-depth",
        "--multiline",
        "--no-config",
        "--no-filename",
        "--no-heading",
        "--no-ignore",
        "--no-ignore-vcs",
        "--no-line-number",
        "--no-messages",
        "--null",
        "--only-matching",
        "--passthru",
        "--pcre2",
        "--quiet",
        "--regexp",
        "--replace",
        "--smart-case",
        "--sort",
        "--sortr",
        "--stats",
        "--text",
        "--threads",
        "--trim",
        "--type",
        "--type-not",
        "--unrestricted",
        "--vimgrep",
        "--with-filename",
        "--word-regexp",
    ]),
    valued: &[
        "-A",
        "-B",
        "-C",
        "-M",
        "-T",
        "-e",
        "-f",
        "-g",
        "-j",
        "-m",
        "-r",
        "-t",
        "--after-context",
        "--before-context",
        "--color",
        "--context",
        "--file",
        "--glob",
        "--iglob",
        "--max-columns",
        "--max-count",
        "--max-depth",
        "--regexp",
        "--replace",
        "--sort",
        "--sortr",
        "--threads",
        "--type",
        "--type-not",
    ],
};

/// `git grep`'s flags that only change what it searches or how it prints what it found, from its help at 2.54.0.
/// Others run another program, as `-O` and `--textconv` do.
pub(super) const GIT_GREP: Flags = Flags {
    allowed: Some(&[
        "-A",
        "-B",
        "-C",
        "-E",
        "-F",
        "-G",
        "-H",
        "-I",
        "-L",
        "-P",
        "-a",
        "-c",
        "-e",
        "-f",
        "-h",
        "-i",
        "-l",
        "-m",
        "-n",
        "-o",
        "-q",
        "-v",
        "-w",
        "-z",
        "--after-context",
        "--all-match",
        "--and",
        "--basic-regexp",
        "--before-context",
        "--break",
        "--cached",
        "--column",
        "--context",
        "--count",
        "--extended-regexp",
        "--files-with-matches",
        "--files-without-match",
        "--fixed-strings",
        "--full-name",
        "--heading",
        "--ignore-case",
        "--invert-match",
        "--line-number",
        "--max-count",
        "--max-depth",
        "--name-only",
        "--no-index",
        "--not",
        "--only-matching",
        "--or",
        "--perl-regexp",
        "--quiet",
        "--untracked",
        "--word-regexp",
    ]),
    valued: &[
        "-A",
        "-B",
        "-C",
        "-e",
        "-f",
        "-m",
        "--after-context",
        "--before-context",
        "--context",
        "--max-count",
        "--max-depth",
    ],
};

/// What a program is given beside its flags.
#[derive(Default)]
pub(super) struct Given<'a> {
    /// The flags it's given, each by its name, such as `-n` or `--files`.
    flags: Vec<String>,
    /// Its words that aren't flags or their values: the files it's given, and a search's pattern where no flag gives
    /// it.
    pub(super) operands: Vec<&'a str>,
    /// How many of `operands` come before a `--`, which is all of them without one.
    pub(super) before_dashes: usize,
    /// The patterns a flag gives it: one `-e` or `--regexp` gives, or `None` for those `-f` or `--file` reads from a
    /// file.
    pub(super) patterns: Vec<Option<&'a str>>,
}

/// What `words`, a program's words after its name, give it, read by its `flags`. `None` where it's given a flag it
/// may not be, or a flag that takes a value without one.
pub(super) fn given<'a>(words: &[&'a str], flags: &Flags) -> Option<Given<'a>> {
    let mut given = Given::default();
    let mut words = words.iter().copied();
    while let Some(word) = words.next() {
        if word == "--" {
            given.before_dashes = given.operands.len();
            given.operands.extend(words);
            return Some(given);
        }
        if let Some(long) = word.strip_prefix("--") {
            let (name, joined) = long.split_once('=').map_or((long, None), |(name, value)| (name, Some(value)));
            let flag = format!("--{name}");
            flags.allows(&flag).then_some(())?;
            if flags.takes_value(&flag) {
                let value = joined.map_or_else(|| words.next(), Some)?;
                given.record(&flag, value);
            }
            given.flags.push(flag);
        } else if let Some(short) = word.strip_prefix('-').filter(|short| !short.is_empty()) {
            // `-20`, as `head` and `tail` take a count of lines.
            if short.chars().all(|c| c.is_ascii_digit()) && flags.allowed.is_none() {
                continue;
            }
            for (at, c) in short.char_indices() {
                let flag = format!("-{c}");
                flags.allows(&flag).then_some(())?;
                given.flags.push(flag.clone());
                if flags.takes_value(&flag) {
                    let joined = &short[at + c.len_utf8()..];
                    let value = if joined.is_empty() { words.next()? } else { joined };
                    given.record(&flag, value);
                    break;
                }
            }
        } else {
            given.operands.push(word);
        }
    }
    given.before_dashes = given.operands.len();
    Some(given)
}

impl Flags {
    fn allows(&self, flag: &str) -> bool {
        self.allowed.is_none_or(|allowed| allowed.contains(&flag))
    }

    fn takes_value(&self, flag: &str) -> bool {
        self.valued.contains(&flag)
    }
}

impl<'a> Given<'a> {
    pub(super) fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|given| given == flag)
    }

    /// Its one operand, where it's given one.
    pub(super) fn one_operand(&self) -> Option<&'a str> {
        match self.operands.as_slice() {
            [operand] => Some(operand),
            _ => None,
        }
    }

    fn record(&mut self, flag: &str, value: &'a str) {
        match flag {
            "-e" | "--regexp" => self.patterns.push(Some(value)),
            "-f" | "--file" => self.patterns.push(None),
            _ => {}
        }
    }
}
