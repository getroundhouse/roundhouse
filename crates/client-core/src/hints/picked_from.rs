//! What hint words are picked from, taken out of the records and the chat it's in, so it can be kept as it was.

use hint_words::{Asked, Choice, PickedFrom, Places, Proposal};
use protocol::capability::asking::Question;
use protocol::model::{ProposalQuestion, Proposed};

use super::{Around, Workspace};
use crate::session_log::{Position, SessionLog};

pub(super) fn around(around: &Around<'_>) -> PickedFrom {
    let workspace = around.workspace.as_ref();
    PickedFrom {
        workspace: workspace.map(in_workspace),
        changed: workspace.map_or_default(|workspace| paths(&workspace.changed)),
        chat: around.chat.map_or_default(loaded),
        ..PickedFrom::default()
    }
}

pub(super) fn paths(paths: &[&str]) -> Vec<String> {
    paths.iter().map(ToString::to_string).collect()
}

pub(super) fn question(question: &Question) -> Asked {
    let options = question
        .options
        .iter()
        .map(|option| Choice { label: option.label.clone(), description: option.description.clone() });
    Asked { header: question.header.clone(), text: question.text.clone(), options: options.collect() }
}

pub(super) fn proposal(proposed: &Proposed) -> Proposal {
    Proposal {
        title: proposed.title.clone(),
        questions: proposed.questions.iter().map(proposal_question).collect(),
        markdown: proposed.markdown().map(str::to_string).collect(),
    }
}

fn proposal_question(question: &ProposalQuestion) -> Asked {
    let options = question
        .options
        .iter()
        .map(|option| Choice { label: option.label.clone(), description: Some(option.description.clone()) });
    Asked { header: Some(question.header.clone()), text: question.text.clone(), options: options.collect() }
}

fn in_workspace(workspace: &Workspace<'_>) -> hint_words::Workspace {
    let shown = workspace.shown;
    let record = shown.workspace();
    let mut branches = vec![record.branch.to_string()];
    if let Some(known) = shown.all().workspace_branches(record) {
        branches.extend(known.checked_out().into_iter().chain(&known.had).map(|branch| branch.name.to_string()));
    }
    hint_words::Workspace {
        name: record.name.clone(),
        branches,
        repo: shown.all().repo(&record.repo_id).map(|repo| repo.name.clone()),
        commented: shown.comments().iter().rev().map(|comment| comment.path.clone()).collect(),
        pull_request: workspace.pull_request.map(|pull_request| pull_request.summary.about.title.clone()),
    }
}

/// The places of the events `log` has loaded, which are the ones its words are picked from.
fn loaded(log: &SessionLog) -> Vec<Places> {
    let place = |row| Position::of_row(row).place().get();
    log.loaded_rows().into_iter().map(|rows| Places { first: place(rows.start), last: place(rows.end - 1) }).collect()
}
