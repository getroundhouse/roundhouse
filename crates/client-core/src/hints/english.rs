//! Filtering out directory boilerplate and everyday English words that the speech model already recognizes without
//! hinting, so hint slots go to proper nouns, acronyms, package names and technical jargon, and telling a lowercase
//! word typed in prose that English rarely uses, which needs a hint, from one it uses often.

use std::collections::HashSet;
use std::sync::LazyLock;

const BOILERPLATE_SEGMENTS: &[&str] = &[
    "src",
    "lib",
    "mod",
    "main",
    "index",
    "test",
    "tests",
    "bin",
    "crates",
    "apps",
    "packages",
    "tools",
    "target",
    "dist",
    "build",
    "docs",
    "spec",
    "guide",
    "vendor",
    "third_party",
    "node_modules",
    ".github",
    ".context",
    "start",
    "master",
    "origin",
    "head",
    "users",
    "home",
    "tmp",
    "var",
];

/// Everyday English words that frequently appear as file stems, constant parts, clause starters or single-word code
/// spans, where boosting adds no accuracy and can tip homophones (`"create"` → `"crate"`, `"feet"` → `"feat"`). Words
/// like `"pane"` stay out on purpose so compound hints such as `"workspace pane"` still win over `"workspace pain"`.
const COMMON_WORDS_LIST: &str = "\
a active actual after again agent agents all allow already also although always an and another answer answers any \
archive archived are around as assemble at attached attachments audio back be beam because before between both bottom \
branch branches boost build built button buttons by cache call calls can cancel cancelling capture card cards change \
changed changes chat chats check checked checks choice choices clear client close closed code collect color colors \
column columns commit commits common complete confirm context contextual continue copy count crate create created \
current currently data date default delete deleted deny desktop detail details dialog diff directory do does done \
draft drafts drop drops during each edit empty encoded end ended engine english enter error errors escape even every \
event events exact example examples existing fail failed fake false fast feat feet field fields file files filter \
final find finish first five fix fixes focus folder folders follow following footer for form found frame frames from \
front full function functions further general get given go goes gone good graph gray green grey group groups guides \
had hand handle has have header heard hearing heavy height helper helpers here hide high hint hints history hold \
holding how hub i icon icons identifiers if ignore ignored impl in info initial input inputs insert inside install \
instead into is issue issues it its item items jargon join joins keep keeping kept key keys kind label labels language \
large last late later layout left length let level levels line linear lines link links list lists live load loaded \
loading local lock locked log logs long look low made main make makes many mark marks match matches max may memory \
menu menus merge merged message messages middle min minute minutes missing mixed mode model models module modules more \
most move moved much must my name named names natural near need needed needs neither never new next no none normal not \
note notes nothing now number numbers numerical of off often old on once one only open opened opening option options \
or order ordinary other our out outline output over own page pages panel part partial parts pass passed paste patch \
path paths pause pauses per performance person phrase phrases piece pieces place plain plan poll portable possible \
practical prefix press previous private process product project projects prompt prompts proper prose public pull \
punctuation purple push put question questions queue queued quick quit raw read readiness reading ready real reason \
recent reconnect reconnecting reconnects record recorded recording records red refresh release remaining remove render \
replace reply repo report request requests reset resource restore result results retry return reusable review right \
rollback root route row rows rule rules run running runs rust safe same save saved say saying said scale score scores \
screen screens scroll search second section see seen select selected selection send sentence sentences separate server \
session sessions set setting settings several share shared shell short should show showing shown side sidebar sides \
simple since single sink site size skip small so some sort sound sounds source sources space spans speak speaking \
specific speech split stand standard start started state status step steps still stop stopped store stream string \
strings struct structure style subagent submit summary support supported switch symbol symbols sync syntax system tab \
table tables tabs tail take taken target task tasks term terms test tested testing tests text texts than that the \
their them theme then there these they thing things this those thread threads three through time times title to today \
toggle token tokens told too top total touch track tree true try turn turning turns two type typed types typing under \
undo unknown unless until unverified up update updated updates use used user users using usual value values verified \
version view views voice wait waiting waits want was watch watched way ways we web well what when where whether which \
while who whole why wide width will window windows with within without word words work worker workers working works \
workspace workspaces write writing written wrong yes yet you your zero";

static COMMON_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| COMMON_WORDS_LIST.split_whitespace().collect());

/// The words English uses most, as `common-words/README.md` says, past which the model starts to get a word wrong
/// unhinted.
static COMMONEST: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| include_str!("common-words/words.txt").lines().collect());

/// Whether `word`, typed in lower case, is one the model may not spell unhinted: letters and digits, in parts between
/// hyphens or dots, such as `tokio`, `ingress-nginx` or `ntfy.sh`, with a part that has letters and isn't among the
/// words English uses most. A contraction or a possessive, such as "doesn't", is a common word whose parts the counts don't
/// hold, so a word with an apostrophe isn't.
pub(super) fn is_rare_lowercase(word: &str) -> bool {
    let fits = word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.'));
    fits && word.split(['-', '.']).any(|part| part.chars().any(|c| c.is_ascii_lowercase()) && !COMMONEST.contains(part))
}

pub(super) fn is_boilerplate(segment: &str) -> bool {
    BOILERPLATE_SEGMENTS.iter().any(|b| b.eq_ignore_ascii_case(segment))
}

pub(super) fn is_common_word(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    COMMON_WORDS.contains(lower.as_str())
}

/// True when `phrase` isn't a single-word mixed-case identifier (`SQLite`, `WorkspacePane`) and every word it splits
/// into is an everyday English word the speech model already knows (including all-caps constants like `MAX` or `KEY`,
/// while acronyms like `GPUI`, `LSTM` and `CPU` aren't in `COMMON_WORDS` and stay).
pub(super) fn all_common_english(phrase: &str) -> bool {
    let single_mixed_case = !phrase.contains(' ')
        && !phrase.contains('_')
        && phrase.chars().skip(1).any(char::is_uppercase)
        && phrase.chars().any(char::is_lowercase);
    if single_mixed_case || phrase.contains('-') {
        return false;
    }
    let mut any = false;
    for part in phrase.split([' ', '_']).filter(|part| !part.is_empty()) {
        any = true;
        if !is_common_word(part) {
            return false;
        }
    }
    any
}
