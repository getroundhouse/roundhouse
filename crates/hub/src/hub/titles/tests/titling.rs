//! A chat in a workspace of its own, whose worker's stand-in answers a call for a title as a test says, and what the
//! hub records of their titles.

use std::path::Path;
use std::sync::Arc;

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AttachmentId, Session, SessionId, TitleSource, Workspace};
use protocol::prompt::Prompt;
use protocol::worker::{TitleFor, WorkerRequest};

use crate::attachment_files::FileFor;
use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_chat, hub_with_stand_in_workers, titling};
use crate::store::AddedBy;

/// What a stand-in's agent answers when it's asked for a title, as the worker does once its CLI has answered.
pub(super) fn answering(title: &str) -> impl Fn(&TitleFor) -> Result<String, ApiError> + Send + 'static {
    let title = title.to_string();
    move |_| Ok(title.clone())
}

/// What a worker answers when its agent's CLI gives no title, as when it isn't logged in.
pub(super) fn no_title() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the CLI gave no title")
}

/// A hub whose one chat, in a workspace of its own, has been sent nothing, and what its workers have been asked. Until
/// a test says how the workspace's worker answers a call for a title, it answers one as it answers what it doesn't
/// know, which the hub can't read as a title.
pub(super) struct Named {
    pub(super) hub: Arc<Hub>,
    pub(super) asked: Arc<Asked>,
    pub(super) made: Workspace,
    pub(super) chat: Session,
}

impl Named {
    pub(super) async fn new(dir: &Path) -> Self {
        let (hub, asked) = hub_with_stand_in_workers(dir);
        let (made, chat) = a_chat(&hub).await;
        Self { hub, asked, made, chat }
    }

    /// Has the workspace's worker answer each call for a title as `title` says, from now on.
    pub(super) fn answers(&self, title: impl Fn(&TitleFor) -> Result<String, ApiError> + Send + 'static) {
        titling(&self.hub, &self.made.id, &self.asked, title);
    }

    pub(super) async fn prompt(&self, chat: &SessionId, prompt: Prompt<AttachmentId>) {
        self.hub.session_prompt(chat.clone(), None, prompt, None).await.unwrap();
    }

    /// Another chat of Claude's in the workspace.
    pub(super) async fn another_chat(&self) -> Session {
        let unchanged = protocol::capability::settings::Settings::default();
        let agent = protocol::model::AgentKind::Claude;
        self.hub.session_create(self.made.id.clone(), agent, unchanged, None).await.unwrap()
    }

    /// Adds a file to the chat for a prompt to carry. An image is enough of a PNG for anything that only looks at how
    /// it starts, which is all the hub does.
    pub(super) async fn add(&self, name: &str, media_type: MediaType) -> Attachment {
        let data = if media_type == MediaType::Png { png() } else { b"error at line 42".to_vec() };
        let (kept_for, id) = (FileFor::Session(self.chat.id.clone()), AttachmentId::from(name.replace('.', "-")));
        let data = AttachmentBytes::new(data);
        self.hub.file_add(kept_for, id, name.into(), media_type, data, AddedBy::Client).await.unwrap()
    }

    pub(super) fn workspace(&self) -> Workspace {
        self.hub.store.get_workspace(&self.made.id).unwrap()
    }

    pub(super) fn session(&self, id: &SessionId) -> Session {
        self.hub.store.get_session(id).unwrap().session
    }

    /// Each call for a title the workspace's worker has been asked, oldest first.
    pub(super) fn calls(&self) -> Vec<TitleFor> {
        self.asked
            .requests(|request| if let WorkerRequest::TitleFor(call) = request { Some(call.clone()) } else { None })
    }

    /// Once the hub has made `count` calls for a title.
    pub(super) async fn asked_for(&self, count: usize) {
        let made = format!("timed out waiting for {count} calls for a title");
        waiting::task::until(TIMEOUT, made, || self.calls().len() >= count).await;
    }

    /// Once the chat `id` has a generated title, which the hub saves after its workspace's, so the workspace's is
    /// settled too.
    pub(super) async fn generated(&self, id: &SessionId) {
        let titled = || self.session(id).title_source == TitleSource::Generated;
        waiting::task::until(TIMEOUT, "timed out waiting for the chat's generated title", titled).await;
    }
}

/// What a call for a title carried of each image: what it was, and its bytes.
pub(super) fn images(call: &TitleFor) -> Vec<(Attachment, Vec<u8>)> {
    call.images.iter().map(|image| (image.attachment.clone(), image.data.as_slice().to_vec())).collect()
}

/// The bytes [`Named::add`] gives an image.
pub(super) fn png() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.resize(300, 7);
    png
}

/// Lets go of the calls for a title that [`held`] answers, once a test has done what it does meanwhile.
pub(super) struct LetGo(std::sync::mpsc::Sender<()>);

impl LetGo {
    pub(super) fn now(&self) {
        self.0.send(()).unwrap();
    }
}

/// What a stand-in's agent answers when it's asked for a title, as [`answering`] does, but only once the test lets it
/// go, so the call is under way while the test asks the hub for something else, as a CLI's call can take seconds. It
/// holds the stand-in's task, as a worker's call holds its CLI, which only a test on more than one thread can do. A
/// test that fails first lets it go as it ends, and one that never lets it go has it answer that it waited too long,
/// so neither hangs. That's twice as long as a test gives a rename, so a rename that waits for the call fails first.
pub(super) fn held(title: &str) -> (LetGo, impl Fn(&TitleFor) -> Result<String, ApiError> + Send + 'static) {
    let (let_go, waiting) = std::sync::mpsc::channel();
    let title = title.to_string();
    let answer = move |_: &TitleFor| {
        tokio::task::block_in_place(|| waiting.recv_timeout(TIMEOUT * 2))
            .map_err(|_| ApiError::new(ErrorCode::Worker, "the test never let the call for a title go"))?;
        Ok(title.clone())
    };
    (LetGo(let_go), answer)
}

/// What the hub answers a rename with while a call for a title is under way. One that waited for the call, or for
/// anything the call holds, would answer only once the test let the call go, so the test fails here first, and says so.
pub(super) async fn within<T>(renaming: impl Future<Output = T>) -> T {
    tokio::time::timeout(TIMEOUT, renaming).await.expect("the rename waited for the call for a title")
}
