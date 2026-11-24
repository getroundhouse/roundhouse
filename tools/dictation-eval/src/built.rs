//! The records a case's field is around, made with `protocol`'s builders as a hub would send them, and the field
//! the app would make of them.

use client_core::hints::{Around, Field, Workspace};
use client_core::records::{HubRecords, InWorkspace};
use client_core::session_log::SessionLog;
use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
use protocol::event::{ContentBlock, EventBody};
use protocol::fixtures;
use protocol::model::{Place, ProposalOption, ProposalQuestion, Proposed, PullRequest, Records, Repo, Rev, Seq};
use protocol::prompt::{Prompt, PromptFrom};

use crate::case::{Case, ChatEntry, FieldKind, Invalid, ProposalCase, QuestionCase, WorkspaceCase};

/// What a case's field is around, kept so the field can borrow it.
pub(crate) struct Built<'c> {
    case: &'c Case,
    shown: Option<InWorkspace>,
    pull_request: Option<PullRequest>,
    chat: Option<SessionLog>,
    question: Option<Question>,
    proposal: Option<Proposed>,
}

impl<'c> Built<'c> {
    pub(crate) fn new(case: &'c Case) -> Self {
        let workspace = case.workspace.as_ref();
        Self {
            case,
            shown: workspace.map(in_workspace),
            pull_request: workspace.and_then(|w| w.pull_request.as_deref()).map(pull_request),
            chat: (!case.chat.is_empty()).then(|| chat(&case.chat)),
            question: case.question.as_ref().map(question),
            proposal: case.proposal.as_ref().map(proposal),
        }
    }

    /// The field, as the app makes it from what's around it, unless the case lacks what its kind of field needs: a
    /// composer its workspace, an answer its question, a note its proposal, a comment its comment, and a new
    /// workspace its repositories.
    pub(crate) fn field(&self) -> Result<Field<'_>, Invalid> {
        let case = self.case;
        let typed = case.typed.as_str();
        let needs = |table| Invalid::Needs(case.field, table);
        Ok(match case.field {
            FieldKind::Composer => Field::Composer {
                typed,
                attached: &case.attached,
                open_file: case.open_file.as_deref(),
                around: Around {
                    workspace: Some(self.workspace().ok_or(needs("workspace"))?),
                    chat: self.chat.as_ref(),
                },
            },
            FieldKind::Answer => Field::Answer {
                typed,
                question: self.question.as_ref().ok_or(needs("question"))?,
                around: self.around(),
            },
            FieldKind::ProposalNote => {
                let proposal = self.proposal.as_ref().ok_or(needs("proposal"))?;
                Field::ProposalNote { typed: Some(typed), proposal, around: self.around() }
            }
            FieldKind::NewWorkspace => {
                let repos = case.repos.as_ref().ok_or(needs("repos"))?;
                Field::NewWorkspace {
                    typed,
                    attached: &case.attached,
                    chosen_repo: repos.chosen.as_deref(),
                    repos: repos.offered.iter().map(String::as_str).collect(),
                }
            }
            // The lines it's on aren't hinted yet.
            FieldKind::DiffComment => Field::DiffComment {
                typed,
                path: &case.comment.as_ref().ok_or(needs("comment"))?.path,
                changed: self.changed().collect(),
            },
        })
    }

    fn around(&self) -> Around<'_> {
        Around { workspace: self.workspace(), chat: self.chat.as_ref() }
    }

    fn workspace(&self) -> Option<Workspace<'_>> {
        let shown = self.shown.as_ref()?;
        Some(Workspace { shown, pull_request: self.pull_request.as_ref(), changed: self.changed().collect() })
    }

    fn changed(&self) -> impl Iterator<Item = &str> {
        self.case.workspace.iter().flat_map(|w| &w.changed).map(String::as_str)
    }
}

fn in_workspace(case: &WorkspaceCase) -> InWorkspace {
    let mut workspace = fixtures::workspace("w1");
    workspace.name.clone_from(&case.name);
    workspace.branch = case.branch.as_str().into();
    workspace.repo_id = "r1".into();
    let repo = Repo { name: case.repo.clone(), rev: Rev::new(1), ..fixtures::repo("r1") };
    let mut records = HubRecords::default();
    records.apply_list(Records { repos: vec![repo], workspaces: vec![workspace], ..Records::default() });
    records.snapshot().in_workspace(&"w1".into()).unwrap_or_else(|| unreachable!("the workspace was just listed"))
}

fn pull_request(title: &str) -> PullRequest {
    let mut pull_request = fixtures::pull_request(1);
    title.clone_into(&mut pull_request.summary.said.about.title);
    pull_request
}

fn chat(entries: &[ChatEntry]) -> SessionLog {
    let mut log = SessionLog::new(Place::default());
    for (seq, entry) in (1..).zip(entries) {
        let body = match entry {
            ChatEntry::Person(text) => EventBody::UserPrompt {
                client_msg_id: None,
                prompt: Prompt::new(text.as_str()),
                from: PromptFrom::Person,
            },
            ChatEntry::Agent(text) => assistant(ContentBlock::Text { text: text.clone(), images: Vec::new() }),
            ChatEntry::Call(call) => {
                assistant(ContentBlock::Call { id: format!("call-{seq}").into(), call: call.clone() })
            }
        };
        log.apply_event(fixtures::event(Seq::new(seq), body));
    }
    log
}

fn assistant(block: ContentBlock) -> EventBody {
    EventBody::Assistant { message_id: None, subagent: None, model: None, blocks: vec![block], first_index: None }
}

fn question(case: &QuestionCase) -> Question {
    let option = |option: &crate::case::QuestionOptionCase| QuestionOption {
        label: option.label.clone(),
        description: option.description.clone(),
        preview: None,
    };
    Question {
        text: case.text.clone(),
        header: case.header.clone(),
        options: case.options.iter().map(option).collect(),
        choose: Choose::One,
        typed: TypedAnswer::Yes,
    }
}

fn proposal(case: &ProposalCase) -> Proposed {
    let questions = case.questions.iter().map(|q| ProposalQuestion {
        header: q.header.clone(),
        text: q.text.clone(),
        details: q.details.clone(),
        options: q
            .options
            .iter()
            .map(|o| ProposalOption {
                label: o.label.clone(),
                description: o.description.clone(),
                body: o.body.clone(),
                letter: None,
            })
            .collect(),
        recommended: None,
    });
    Proposed { title: case.title.clone(), context: case.context.clone(), questions: questions.collect() }
}
