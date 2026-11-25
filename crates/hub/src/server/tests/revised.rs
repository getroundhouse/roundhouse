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
fn of_reply(response: &ClientResponse) -> Vec<(String, Rev)> {
    records_in(response).iter().map(record).collect()
}

/// The records a reply is, or holds.
fn records_in(response: &ClientResponse) -> Vec<Record> {
    use ClientResponse as C;
    match response {
        C::HubSettings(settings) => vec![Record::HubSettings(settings.clone())],
        C::NewWorkspaceDraft(draft) => vec![Record::NewWorkspaceDraft(draft.clone())],
        C::NewWorkspaceMade(made) => {
            vec![Record::Workspace(made.workspace.clone()), Record::Session(made.session.clone())]
        }
        C::Repo(repo) => vec![Record::Repo(repo.clone())],
        C::Repos(repos) => repos.repos.iter().cloned().map(Record::Repo).collect(),
        C::ModelList(model_list) => vec![Record::ModelList(model_list.clone())],
        C::Workspace(workspace) => vec![Record::Workspace(workspace.clone())],
        C::Workspaces(workspaces) => workspaces.workspaces.iter().cloned().map(Record::Workspace).collect(),
        C::Comment(comment) => vec![Record::Comment(comment.clone())],
        C::Comments(comments) => comments.comments.iter().cloned().map(Record::Comment).collect(),
        C::Session(session) => vec![Record::Session(session.clone())],
        C::Sessions(sessions) => sessions.sessions.iter().cloned().map(Record::Session).collect(),
        C::Proposal(proposal) => vec![Record::Proposal(proposal.clone())],
        C::Account(account) => vec![Record::Account(account.clone())],
        C::GithubAccount(account) => vec![Record::GithubAccount(account.clone())],
        C::GithubSignIn(sign_in) => vec![Record::GithubSignIn(sign_in.clone())],
        C::GithubApp(app) => vec![Record::GithubApp(app.clone())],
        C::ToolServer(server) => vec![Record::ToolServer(server.clone())],
        C::Terminal(terminal) => vec![Record::Terminal(terminal.clone())],
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
