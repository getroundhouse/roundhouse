//! What was sent, set against the recordings it came from: those dictated into the same field, workspace and chat since the
//! last thing sent from there.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use kept_recordings::{Kept, SENT, Sent, SentFrom};

use super::became::{Became, became};
use super::recordings::{self, Recording};
use crate::lines;
use crate::score::diff;

/// Lists what became of each recording in `folder`.
pub(crate) fn sent(folder: &Path) -> anyhow::Result<String> {
    let recordings = recordings::read(folder)?;
    let mut sent: Vec<Sent> = lines::read_file(&folder.join(SENT))?;
    sent.sort_by_key(|sent| sent.sent_at_ms);
    Ok(Totals::of(&recordings, &sent).text())
}

/// How many recordings became what, and those whose text wasn't sent as it was written.
#[derive(Default)]
struct Totals {
    as_written: usize,
    marks: usize,
    changed: usize,
    not_there: usize,
    /// Nothing has been sent from there since.
    unsent: usize,
    /// Nothing was written of it.
    unwritten: usize,
    list: String,
}

impl Totals {
    fn of(recordings: &[Recording], sent: &[Sent]) -> Self {
        let mut totals = Self::default();
        // By where each send is in `sent`, which is when it was sent.
        let mut dictated: BTreeMap<usize, Vec<&Recording>> = BTreeMap::new();
        for recording in recordings {
            if recording.finals().trim().is_empty() {
                totals.unwritten += 1;
            } else if let Some(at) = sent.iter().position(|sent| came_from(sent, &recording.kept)) {
                dictated.entry(at).or_default().push(recording);
            } else {
                totals.unsent += 1;
            }
        }
        for (at, recordings) in dictated {
            let written: Vec<String> = recordings.iter().map(|recording| recording.finals()).collect();
            let became = became(&written, &sent[at].text);
            for ((recording, written), became) in recordings.iter().zip(&written).zip(became) {
                totals.add(&recording.kept.id, written, &became);
            }
        }
        totals
    }

    fn add(&mut self, id: &str, written: &str, became: &Became) {
        let (count, sent_as) = match became {
            Became::AsWritten => {
                self.as_written += 1;
                return;
            }
            Became::Marks(sent_as) => (&mut self.marks, Some(sent_as)),
            Became::Changed(sent_as) => (&mut self.changed, Some(sent_as)),
            Became::NotThere => (&mut self.not_there, None),
        };
        *count += 1;
        let _ = writeln!(self.list, "\n{id}\n  wrote: {written}");
        let _ = match sent_as {
            Some(sent_as) => writeln!(self.list, "  sent: {sent_as}\n  {}", diff(written, sent_as)),
            None => writeln!(self.list, "  not in what was sent"),
        };
    }

    fn text(&self) -> String {
        let Self { as_written, marks, changed, not_there, unsent, unwritten, list } = self;
        format!(
            "{as_written} were sent as they were written, {marks} with other capitals or marks, {changed} changed, and \
             {not_there} weren't in what was sent. Nothing has been sent since {unsent}, and nothing was written of \
             {unwritten}.\n{list}"
        )
    }
}

/// Whether `sent` was sent from where `kept` was dictated, after it began.
fn came_from(sent: &Sent, kept: &Kept) -> bool {
    let from = SentFrom {
        field: kept.field,
        workspace: kept.workspace.clone(),
        chat: kept.chat.clone(),
        said_out_loud: kept.said_out_loud,
    };
    sent.sent_at_ms >= kept.started_at_ms && sent.from == from
}
