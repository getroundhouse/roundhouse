//! All files: every file in the workspace's worktree, tracked, new or ignored, in folders that start closed. A folder
//! git ignores whole, such as `target`, is read only when it's opened, since it can hold hundreds of thousands of
//! files. Clicking a file shows it in the workspace's file tab as it is in the worktree, rather than its diff.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use client_core::{Client, ClientError};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, EventEmitter, Hsla, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, uniform_list,
};
use protocol::client::ErrorCode;
use protocol::model::{WorkspaceId, WorktreeFiles};

use crate::file_icons::folder_icon;
use crate::file_tree::{List, Row, chain, file_frame, folder_row, split};
use listed::Listed;

mod listed;

#[derive(Clone, Debug)]
pub enum AllFilesEvent {
    /// Show this file in the file tab.
    Open(SharedString),
    Error(String),
    /// The hub is too old to list files, which All files says in the list's place.
    Unsupported,
}

/// What All files has to show.
enum Listing {
    Loading,
    Listed(Listed),
    /// The hub is older than the app, and can't list files.
    Unsupported,
    /// It couldn't, and there was no list from before to keep showing.
    Failed(String),
}

pub struct AllFiles {
    client: Client,
    workspace_id: WorkspaceId,
    /// What the hub last said is in the worktree, shared with the thread that lays it out rather than copied.
    worktree: Option<Arc<WorktreeFiles>>,
    /// What's in each folder read since, by its path, for the folders open.
    read: BTreeMap<String, Arc<WorktreeFiles>>,
    listing: Listing,
    /// Folders opened, by path. They start closed, since a whole repository open at once is too much to look through.
    open: HashSet<String>,
    /// The file the file tab shows, as the pane says, which its row marks.
    showing: Option<SharedString>,
    /// The rows as last laid out, which the list draws from as it scrolls. They change only when the files or the open
    /// folders do.
    rows: Vec<Row>,
    /// How many times the worktree has been listed, and its files laid out, so only the newest of each is shown.
    asks: u64,
    builds: u64,
    /// The folders being read, each by which read is the newest, so an older one that comes back later is dropped.
    reading: HashMap<String, u64>,
    reads: u64,
}

impl EventEmitter<AllFilesEvent> for AllFiles {}

impl AllFiles {
    /// Lists nothing until [`Self::refresh`], so a workspace whose All files never shows never asks.
    pub fn new(client: Client, workspace_id: WorkspaceId) -> Self {
        Self {
            client,
            workspace_id,
            worktree: None,
            read: BTreeMap::new(),
            listing: Listing::Loading,
            open: HashSet::new(),
            showing: None,
            rows: Vec::new(),
            asks: 0,
            builds: 0,
            reading: HashMap::new(),
            reads: 0,
        }
    }

    /// Carries on on a new connection to the hub. What was being asked on the old one is dropped, and asked again on
    /// this one with the next [`Self::refresh`].
    pub fn reconnect(&mut self, client: &Client) {
        self.client = client.clone();
        self.asks += 1;
        self.reading.clear();
    }

    /// Whether the folder at `path` is open.
    #[cfg(feature = "testing")]
    pub fn is_open(&self, path: &str) -> bool {
        self.open.contains(path)
    }

    /// Whether the hub has listed the files, so they show.
    #[cfg(feature = "testing")]
    pub fn is_listed(&self) -> bool {
        matches!(self.listing, Listing::Listed(_))
    }

    /// Lists the files again, and reads again each folder that's open and in view and is read when it's opened,
    /// keeping the files shown and the folders open until the new list is ready.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.asks += 1;
        let (ask, client, id) = (self.asks, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let listed = cx.background_spawn(async move { client.workspace_files(&id) }).await;
            let _ = this.update(cx, |t, cx| {
                if ask != t.asks {
                    return;
                }
                match listed {
                    Ok(files) => {
                        // A folder that's closed is read again when it opens, so what it held is forgotten.
                        t.read.retain(|path, _| t.open.contains(path));
                        let folders: Vec<String> = files.folders.clone();
                        t.worktree = Some(Arc::new(files));
                        t.read_open(&folders, cx);
                        t.lay_out_files(cx);
                    }
                    // Lost with the connection, and listed again once the pane catches up on the next one.
                    Err(ClientError::Disconnected) => {}
                    Err(e) if e.is_unsupported() => {
                        t.listing = Listing::Unsupported;
                        cx.emit(AllFilesEvent::Unsupported);
                    }
                    // A list from before is still worth showing.
                    Err(_) if t.worktree.is_some() => {}
                    Err(e) => t.listing = Listing::Failed(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads each of `folders` that's open and in view, so it shows what's in it now.
    fn read_open(&mut self, folders: &[String], cx: &mut Context<Self>) {
        let shown: Vec<String> =
            folders.iter().filter(|folder| self.open.contains(*folder) && self.in_view(folder)).cloned().collect();
        for folder in shown {
            self.read_folder(folder, cx);
        }
    }

    /// Whether every folder `path` is in is open, so its row shows.
    fn in_view(&self, path: &str) -> bool {
        path.match_indices('/').all(|(at, _)| self.open.contains(&path[..at]))
    }

    /// Reads what's in a folder whose files weren't listed, and shows them in it, reading in turn each folder in it
    /// that's open.
    fn read_folder(&mut self, path: String, cx: &mut Context<Self>) {
        self.reads += 1;
        let read = self.reads;
        self.reading.insert(path.clone(), read);
        let (client, id, folder) = (self.client.clone(), self.workspace_id.clone(), path.clone());
        cx.spawn(async move |this, cx| {
            let listed = cx.background_spawn(async move { client.workspace_folder_read(&id, &folder) }).await;
            let _ = this.update(cx, |t, cx| {
                if t.reading.get(&path) != Some(&read) {
                    return;
                }
                t.reading.remove(&path);
                match listed {
                    Ok(files) => {
                        let folders = files.folders.clone();
                        t.read.insert(path, Arc::new(files));
                        t.read_open(&folders, cx);
                    }
                    // Lost with the connection, so the folder stays as it was, and is read again once the pane
                    // catches up on the next one.
                    Err(ClientError::Disconnected) => {}
                    Err(e) => {
                        t.read.remove(&path);
                        t.open.remove(&path);
                        // One that's gone since it was listed goes from the list when it's listed again.
                        if !matches!(&e, ClientError::Api(api) if api.code == ErrorCode::NotFound) {
                            cx.emit(AllFilesEvent::Error(format!("couldn't open {path}: {e}")));
                        }
                    }
                }
                // Once, when the last of the folders being read is in.
                if t.reading.is_empty() {
                    t.lay_out_files(cx);
                }
            });
        })
        .detach();
    }

    /// Lays the files out in their folders again, off the main thread, since a big repository has thousands.
    fn lay_out_files(&mut self, cx: &mut Context<Self>) {
        let Some(worktree) = self.worktree.clone() else { return };
        self.builds += 1;
        let (build, read) = (self.builds, self.read.clone());
        cx.spawn(async move |this, cx| {
            let listed = cx.background_spawn(async move { Listed::new(&worktree, &read) }).await;
            let _ = this.update(cx, |t, cx| {
                if build == t.builds {
                    t.listing = Listing::Listed(listed);
                    t.lay_out();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Marks the row of the file the file tab shows, or none.
    pub fn set_showing(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        self.showing = path;
        cx.notify();
    }

    /// Opens or closes a folder's row, and each folder it stands for. One whose files weren't listed is read each time
    /// it opens, so it shows what's in it now, with each open folder inside it.
    fn toggle_folder(&mut self, path: &str, names: &[String], cx: &mut Context<Self>) {
        let folders = chain(path, names);
        if self.open.contains(path) {
            for folder in &folders {
                self.open.remove(folder);
            }
        } else {
            self.open.extend(folders.iter().cloned());
            // The folders it stands for, and any inside them left open, which come into view with it.
            let inside = format!("{path}/");
            let lazy: Vec<String> = match &self.listing {
                Listing::Listed(listed) => listed
                    .folders
                    .iter()
                    .filter(|folder| folders.contains(*folder) || folder.starts_with(&inside))
                    .cloned()
                    .collect(),
                Listing::Loading | Listing::Unsupported | Listing::Failed(_) => Vec::new(),
            };
            // Reading one goes on to read each open folder in it, so one inside another isn't read again here.
            let outermost: Vec<String> = lazy
                .iter()
                .filter(|folder| {
                    !lazy
                        .iter()
                        .any(|other| folder.starts_with(other.as_str()) && folder[other.len()..].starts_with('/'))
                })
                .cloned()
                .collect();
            self.read_open(&outermost, cx);
        }
        self.lay_out();
        cx.notify();
    }

    fn lay_out(&mut self) {
        let rows = match &self.listing {
            Listing::Listed(listed) => listed.tree.rows(|path| self.open.contains(path)),
            Listing::Loading | Listing::Unsupported | Listing::Failed(_) => Vec::new(),
        };
        self.rows = rows;
    }

    /// The rows in `range`, for the list to draw.
    fn draw_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Listing::Listed(listed) = &self.listing else { return Vec::new() };
        let rows = self.rows.get(range).unwrap_or_default();
        // Every name is in the mono font, a file's as a folder's is, since the list is a tree of paths to browse.
        let mono = cx.theme().mono_font_family.clone();
        rows.iter()
            .filter_map(|row| match row {
                Row::Folder { path, names, depth, open } => {
                    let (path, names) = (path.clone(), names.clone());
                    let row = folder_row(
                        format!("all-folder-{path}"),
                        List::AllFiles,
                        &names,
                        *depth,
                        *open,
                        folder_icon(*open, cx),
                        cx,
                    )
                    .on_click(cx.listener(move |t, _, _, cx| t.toggle_folder(&path, &names, cx)));
                    Some(row.test_support().into_any_element())
                }
                Row::File { index, depth } => {
                    let path = listed.paths.get(*index)?.clone();
                    let showing = self.showing.as_ref() == Some(&path);
                    let name = split(&path).1.to_string();
                    let row = file_frame(format!("all-file-{path}"), &path, *depth, showing, cx)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_xs()
                                .font_family(mono.clone())
                                .child(name),
                        )
                        .on_click(cx.listener(move |_, _, _, cx| cx.emit(AllFilesEvent::Open(path.clone()))));
                    Some(row.test_support().into_any_element())
                }
            })
            .collect()
    }
}

impl Render for AllFiles {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = look::colors(cx).text_muted;
        let truncated = matches!(&self.listing, Listing::Listed(listed) if listed.truncated);
        let body = match &self.listing {
            Listing::Loading => placeholder("Loading…".into(), muted),
            Listing::Unsupported => placeholder("The hub is too old to list files".into(), muted),
            Listing::Failed(message) => placeholder(format!("Couldn't list the files: {message}").into(), muted),
            Listing::Listed(_) if self.rows.is_empty() => placeholder("No files".into(), muted),
            Listing::Listed(_) => {
                uniform_list("all-files", self.rows.len(), cx.processor(|t, range, _, cx| t.draw_rows(range, cx)))
                    .flex_1()
                    .w_full()
                    .py_1()
                    .into_any_element()
            }
        };
        v_flex()
            .size_full()
            .when(truncated, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .child("There are so many files that some aren't shown"),
                )
            })
            .child(body)
    }
}

fn placeholder(text: SharedString, color: Hsla) -> AnyElement {
    div()
        .flex_1()
        .w_full()
        .px_4()
        .flex()
        .items_center()
        .justify_center()
        .text_center()
        .text_color(color)
        .child(text)
        .into_any_element()
}
