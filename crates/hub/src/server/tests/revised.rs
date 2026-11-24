//! That everything a hub sends that a client keeps by revision has one: each record and observation a reply or a frame
//! carries, each removal, and each list. A client of version 5 takes a copy at no revision for one older than any it
//! holds, so a hub that sent one would leave it showing what it held. Every frame a test's [`super::Connection`] reads
//! goes through [`checked`], so every test of what a connection is sent checks this too.

use protocol::client::listed::Listed;
use protocol::client::{ClientResponse, HubFrame, HubNotice, ObservationNotice, RecordNotice, ReplyOutcome};
use protocol::model::{Observation, Record, Records, Rev};

/// `frame`, once it's found to carry no record, observation, removal or list at no revision.
pub(super) fn checked(frame: HubFrame) -> HubFrame {
    let unrevised: Vec<String> =
        revisions(&frame).into_iter().filter(|(_, rev)| *rev == Rev::default()).map(|(what, _)| what).collect();
    assert!(unrevised.is_empty(), "sent at no revision: {unrevised:?}, in {frame:?}");
    frame
}

/// Each revision `frame` carries, with what it's of.
fn revisions(frame: &HubFrame) -> Vec<(String, Rev)> {
    match frame {
        HubFrame::Reply { outcome: ReplyOutcome::Response(response), .. } => of_reply(response),
        HubFrame::Notice { notice, .. } => of_notice(notice),
        HubFrame::Listed { listed, .. } => of_listed(listed),
        HubFrame::Reply { outcome: ReplyOutcome::Error(_), .. }
        | HubFrame::Event { .. }
        | HubFrame::Tail { .. }
        | HubFrame::Delta { .. }
        | HubFrame::Github { .. }
        | HubFrame::Terminal { .. }
        | HubFrame::TerminalPicture { .. } => Vec::new(),
    }
}

fn record(record: &Record) -> (String, Rev) {
    (format!("{:?}", record.key()), record.rev())
}

fn observation(observation: &Observation) -> (String, Rev) {
    (format!("{:?}", observation.key()), observation.rev())
}

fn list(what: &str, rev: Rev) -> (String, Rev) {
    (format!("a list of {what}"), rev)
}

fn of_notice(notice: &HubNotice) -> Vec<(String, Rev)> {
    if let Ok(told) = RecordNotice::try_from(notice.clone()) {
        return vec![match told {
            RecordNotice::Copy(copy) => record(&copy),
            RecordNotice::Removed { record, rev } => (format!("{record:?} removed"), rev),
        }];
    }
    match ObservationNotice::try_from(notice.clone()) {
        Ok(ObservationNotice::Copy(copy)) => vec![observation(&copy)],
        Ok(ObservationNotice::Removed { observation, rev }) => vec![(format!("{observation:?} removed"), rev)],
        // `Resync`, which carries nothing.
        Err(_) => Vec::new(),
    }
}

fn of_records(records: &Records) -> Vec<(String, Rev)> {
    let mut revisions: Vec<_> = records.clone().into_records().map(|copy| record(&copy)).collect();
    revisions.extend(records.observations.iter().map(observation));
    revisions.push(list("every record", records.rev));
    revisions
}

fn of_listed(listed: &Listed) -> Vec<(String, Rev)> {
    match listed {
        Listed::Hub(records) => of_records(records),
        Listed::Session(session) => {
            let mut revisions: Vec<_> = (session.subagents.iter().cloned().map(Record::Subagent))
                .chain(session.proposals.iter().cloned().map(Record::Proposal))
                .chain(session.replies_heard.iter().cloned().map(Record::ReplyHeard))
                .map(|copy| record(&copy))
                .collect();
            revisions.extend(session.observations.iter().map(observation));
            revisions.push(list("a session's records", session.rev));
            revisions
        }
        Listed::Workspace(workspace) => {
            let mut revisions: Vec<_> = workspace.observations.iter().map(observation).collect();
            revisions.push(list("a workspace's observations", workspace.rev));
            revisions
        }
        Listed::Reference(reference) => {
            let mut revisions: Vec<_> = reference.observations.iter().map(observation).collect();
            revisions.push(list("a reference's observations", reference.rev));
            revisions
        }
        Listed::Unknown => Vec::new(),
    }
}

/// Each revision a reply carries, of the records in it and of a list it is.
#[expect(clippy::too_many_lines, reason = "a flat match over every reply, each arm a record, a list or nothing")]
fn of_reply(response: &ClientResponse) -> Vec<(String, Rev)> {
    use ClientResponse as C;
    let each = |records: Vec<Record>| records.iter().map(record).collect();
    match response {
        C::HubSettings(settings) => each(vec![Record::HubSettings(settings.clone())]),
        C::NewWorkspaceDraft(draft) => each(vec![Record::NewWorkspaceDraft(draft.clone())]),
        C::NewWorkspaceMade(made) => {
            each(vec![Record::Workspace(made.workspace.clone()), Record::Session(made.session.clone())])
        }
        C::Repo(repo) => each(vec![Record::Repo(repo.clone())]),
        C::Repos(repos) => each(repos.repos.iter().cloned().map(Record::Repo).collect()),
        C::Records(records) => of_records(records),
        C::ModelList(model_list) => each(vec![Record::ModelList(model_list.clone())]),
        C::Workspace(workspace) => each(vec![Record::Workspace(workspace.clone())]),
        C::Workspaces(workspaces) => each(workspaces.workspaces.iter().cloned().map(Record::Workspace).collect()),
        C::Comment(comment) => each(vec![Record::Comment(comment.clone())]),
        C::Comments(comments) => each(comments.comments.iter().cloned().map(Record::Comment).collect()),
        C::Session(session) => each(vec![Record::Session(session.clone())]),
        C::Sessions(sessions) => each(sessions.sessions.iter().cloned().map(Record::Session).collect()),
        C::Subagents(subagents) => {
            let mut revisions = each(subagents.subagents.iter().cloned().map(Record::Subagent).collect());
            revisions.push(list("a session's subagents", subagents.rev));
            revisions
        }
        C::Proposals(proposals) => {
            let mut revisions = each(proposals.proposals.iter().cloned().map(Record::Proposal).collect());
            revisions.push(list("a session's proposals", proposals.rev));
            revisions
        }
        C::RepliesHeard(heard) => {
            let mut revisions = each(heard.replies.iter().cloned().map(Record::ReplyHeard).collect());
            revisions.push(list("a session's replies heard", heard.rev));
            revisions
        }
        C::Proposal(proposal) => each(vec![Record::Proposal(proposal.clone())]),
        C::Account(account) => each(vec![Record::Account(account.clone())]),
        C::GithubAccount(account) => each(vec![Record::GithubAccount(account.clone())]),
        C::GithubSignIn(sign_in) => each(vec![Record::GithubSignIn(sign_in.clone())]),
        C::GithubApp(app) => each(vec![Record::GithubApp(app.clone())]),
        C::Terminal(terminal) => each(vec![Record::Terminal(terminal.clone())]),
        // What isn't a record, or holds none.
        C::Paired(_)
        | C::Devices(_)
        | C::Pairing(_)
        | C::Device(_)
        | C::Hello(_)
        | C::Ok
        | C::HubStatus(_)
        | C::Agents(_)
        | C::GitStatus(_)
        | C::GitDiff(_)
        | C::ChangedFilesPage(_)
        | C::FileDiff(_)
        | C::SetupLog(_)
        | C::PullRequest(_)
        | C::Events(_)
        | C::References(_)
        | C::ProposalRevisions(_)
        | C::PlacedEvents(_)
        | C::Prompts(_)
        | C::Compactions(_)
        | C::Found(_)
        | C::PromptRecorded(_)
        | C::Attachment(_)
        | C::AttachmentData(_)
        | C::SessionAccount(_)
        | C::GhLogin(_)
        | C::MachineLogins(_)
        | C::GithubAppManifest(_)
        | C::WorktreeFiles(_)
        | C::FileContents(_)
        | C::FileOpened(_)
        | C::FilePages(_)
        | C::FileRange(_)
        | C::TerminalRows(_)
        | C::TerminalBounds(_)
        | C::TerminalText(_)
        | C::TerminalFit(_)
        | C::Subscribed(_) => Vec::new(),
    }
}
