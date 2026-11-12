//! The branch the diff screens review: a made-up change across several folders, as `git diff` gives it.

#![cfg(test)]

use super::worktree::{MODEL, OPENED};

/// The commit the diff is taken from.
pub(crate) const BASE_COMMIT: &str = "3f4c19c8a2d1e0b7f6a5c4d3e2f1a0b9c8d7e6f5";

/// A branch that fixes the badge in the sidebar, and changes the screen that shows it, which git says only differs,
/// and adds another.
pub(crate) const IMAGES: &str = "diff --git a/docs/spec/screens/sidebar.png b/docs/spec/screens/sidebar.png
index 7c1d2e3..8f4a5b6 100644
Binary files a/docs/spec/screens/sidebar.png and b/docs/spec/screens/sidebar.png differ
diff --git a/docs/spec/screens/sidebar-narrow.png b/docs/spec/screens/sidebar-narrow.png
new file mode 100644
index 0000000..2b3c4d5
Binary files /dev/null and b/docs/spec/screens/sidebar-narrow.png differ
";

/// What the branch adds to the file the screens open, after its workspace's ports.
const ADDED: [&str; 3] = [
    "    /// The pull request for its branch, as the hub last saw it. `None` if there isn't one.",
    "    #[serde(default)]",
    "    pub pull_request: Option<PullRequest>,",
];

/// The line the branch adds the pull request on, which a comment is left on.
pub(crate) const PULL_REQUEST_LINE: u32 = 37;

/// The whole branch, with git's three lines either side of each change, as the hub sends it. The file the screens open
/// is the worktree's before the branch changed it, which the hub has as it was in the diff's commit.
pub(crate) fn patch() -> String {
    let before: Vec<&str> = MODEL.lines().collect();
    let at = before.iter().position(|line| line.contains("pub port_base")).unwrap() + 1;
    let (from, to) = (at - 3, (at + 3).min(before.len()));
    let mut patch = format!(
        "diff --git a/{OPENED} b/{OPENED}\nindex 1a2b3c4..5d6e7f8 100644\n--- a/{OPENED}\n+++ b/{OPENED}\n\
         @@ -{},{} +{},{} @@ pub struct Workspace {{\n",
        from + 1,
        to - from,
        from + 1,
        to - from + ADDED.len()
    );
    let line = |sign: char, text: &str| format!("{sign}{text}\n");
    patch.extend(before[from..at].iter().map(|text| line(' ', text)));
    patch.extend(ADDED.iter().map(|text| line('+', text)));
    patch.extend(before[at..to].iter().map(|text| line(' ', text)));
    assert_eq!(u32::try_from(at + ADDED.len()).unwrap(), PULL_REQUEST_LINE);
    patch + REST
}

const REST: &str = r##"diff --git a/apps/desktop/src/sidebar.rs b/apps/desktop/src/sidebar.rs
index 9f8e7d6..c5b4a39 100644
--- a/apps/desktop/src/sidebar.rs
+++ b/apps/desktop/src/sidebar.rs
@@ -37,6 +37,7 @@ pub(crate) fn status_color(status: Option<SessionStatus>, cx: &App) -> Hsla {
 fn workspace_row(app: &AppView, workspace: &Workspace, cx: &mut Context<AppView>) -> impl IntoElement {
     let selected = app.selected.as_ref() == Some(&workspace.id);
     let status = app.sessions.get(&workspace.id).and_then(|s| workspace_status(s));
+    let pull_request = workspace.pull_request.as_ref().map(|pr| format!("#{}", pr.number));
     let archived = workspace.status == WorkspaceStatus::Archived;
     let id = workspace.id.clone();
     let muted = cx.theme().muted_foreground;
@@ -59,6 +60,7 @@ fn workspace_row(app: &AppView, workspace: &Workspace, cx: &mut Context<AppView>) -> impl IntoElement {
                 .when(archived, |d| d.text_color(muted))
                 .child(div().flex_none().size_2().rounded_full().bg(dot))
                 .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(workspace.name.clone()))
+                .children(pull_request.map(|pr| div().flex_none().text_xs().text_color(muted).child(pr)))
                 .when(archived, |d| d.child(div().flex_none().text_xs().child("Restore"))),
         )
 }
diff --git a/apps/desktop/tests/ui/sidebar.rs b/apps/desktop/tests/ui/sidebar.rs
new file mode 100644
index 0000000..ab56be3
--- /dev/null
+++ b/apps/desktop/tests/ui/sidebar.rs
@@ -0,0 +1,19 @@
+//! The sidebar's rows, and what each says about its workspace.
+
+#![cfg(test)]
+
+use gpui_kit::TestAppContext;
+
+use crate::support::{Stack, Ui};
+
+#[gpui_kit::test]
+fn a_workspace_that_isnt_open_shows_its_pull_request(cx: &mut TestAppContext) {
+    let stack = Stack::new();
+    let mut ui = Ui::open(cx, &stack);
+    let (first, second) = (ui.new_workspace(), ui.new_workspace());
+
+    stack.set_pull_request(&first, 182, "OPEN");
+
+    ui.wait("its row to show it", |ui| ui.row_pull_request(&first) == Some(182));
+    assert_eq!(ui.view().selected.as_ref(), Some(&second));
+}
diff --git a/crates/hub/src/hub.rs b/crates/hub/src/hub.rs
index 3c3c541..a0adf5a 100644
--- a/crates/hub/src/hub.rs
+++ b/crates/hub/src/hub.rs
@@ -1,6 +1,7 @@
 //! The hub: what it owns, and the requests that change it.
 
 mod names;
+mod pull_requests;
 mod sessions;
 mod workspaces;
 
@@ -11,6 +12,7 @@ use protocol::model::{Workspace, WorkspaceId};
 pub struct Hub {
     store: Arc<Store>,
     workers: Workers,
+    pull_requests: pull_requests::Watcher,
 }
 
 impl Hub {
diff --git a/crates/hub/src/hub/pull_requests.rs b/crates/hub/src/hub/pull_requests.rs
new file mode 100644
index 0000000..4e75fc0
--- /dev/null
+++ b/crates/hub/src/hub/pull_requests.rs
@@ -0,0 +1,27 @@
+//! Keeps each workspace's pull request current, by asking its worker every so often, so every client hears it
+//! from the feed rather than asking GitHub itself.
+
+use std::time::Duration;
+
+use protocol::model::{PullRequest, WorkspaceId};
+
+/// Often enough that a merge shows within a minute, and rare enough to stay well inside GitHub's rate limit.
+const EVERY: Duration = Duration::from_secs(45);
+
+pub(crate) struct Watcher {
+    last: HashMap<WorkspaceId, Option<PullRequest>>,
+}
+
+impl Watcher {
+    /// Asks again for each workspace, and says which changed.
+    pub(crate) async fn check(&mut self, workers: &Workers) -> Vec<WorkspaceId> {
+        let mut changed = Vec::new();
+        for (id, worker) in workers.iter() {
+            let found = worker.pull_request().await;
+            if self.last.insert(id.clone(), found.clone()) != Some(found) {
+                changed.push(id.clone());
+            }
+        }
+        changed
+    }
+}
diff --git a/crates/protocol/src/client.rs b/crates/protocol/src/client.rs
index 8a66ca8..bd6ca1b 100644
--- a/crates/protocol/src/client.rs
+++ b/crates/protocol/src/client.rs
@@ -5,4 +5,5 @@ pub enum HubNotice {
     Workspace(Workspace),
     Session(Session),
     Comment(DiffComment),
+    PullRequest(WorkspaceId, Option<PullRequest>),
 }
diff --git a/crates/worker/src/git.rs b/crates/worker/src/git.rs
index 7e342f5..95eb516 100644
--- a/crates/worker/src/git.rs
+++ b/crates/worker/src/git.rs
@@ -2,15 +2,17 @@
 
 use protocol::model::PullRequest;
 
+/// How long `gh` gets before the answer counts as none, so a slow network doesn't hold up the hub's next check.
+const GH_TIMEOUT: Duration = Duration::from_secs(10);
+
 /// The pull request for the current branch. `None` covers both "there isn't one" and "`gh` isn't
 /// installed or isn't logged in", because neither should stop anyone working.
 pub async fn pull_request(path: &str) -> Option<PullRequest> {
-    let output = Command::new("gh")
+    let asking = Command::new("gh")
         .args(["pr", "view", "--json", "number,url,title,state,isDraft"])
         .current_dir(path)
-        .output()
-        .await
-        .ok()?;
+        .output();
+    let output = tokio::time::timeout(GH_TIMEOUT, asking).await.ok()?.ok()?;
     if !output.status.success() {
         return None;
     }
diff --git a/docs/spec/sidebar.md b/docs/spec/sidebar.md
index 79cc84d..c76e03e 100644
--- a/docs/spec/sidebar.md
+++ b/docs/spec/sidebar.md
@@ -1,4 +1,4 @@
 # Sidebar
 
 - **A workspace's mark speaks for its open chat that most needs attention,** so a chat waiting on approval is visible from anywhere.
-- **The sidebar shows every workspace's pull request.** Only a workspace that's open knows its pull request.
+- **The sidebar shows every workspace's pull request,** as the hub last saw it, whether or not the workspace is open.
"##;
