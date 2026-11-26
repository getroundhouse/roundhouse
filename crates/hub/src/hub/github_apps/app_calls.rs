use std::sync::Arc;

use protocol::client::github_apps::GithubManifestCode;
use protocol::client::{ApiError, HubNotice};
use protocol::model::{GithubApp, GithubAppId, GithubAppSaid, RecordKey};
use tracing::warn;

use super::super::{Hub, off_thread};
use crate::github_logins::{AppChange, AppRemoved, GithubAccountChange};
use crate::hub::GithubAppSaid as Said;
use crate::store::now_ms;

impl Hub {
    /// Makes an app of your own from the code GitHub sent the person back with, as `GithubAppMake` says: the same code
    /// again finds the app it made, without asking GitHub; otherwise GitHub makes it into the app's key, which the hub
    /// keeps, and tells every client of the app's record. Then it looks for where the app's installed, as the person
    /// asked, and answers with the app's record as that left it, or, when that fails, as it was kept, since the app is
    /// made either way, and every client is told of what the look changed before it failed.
    pub(crate) async fn github_app_make(&self, code: GithubManifestCode) -> Result<GithubApp, ApiError> {
        let (logins, asked) = (Arc::clone(&self.github_logins), code.clone());
        if let Some(app) = off_thread(move || logins.made_from(&asked)).await? {
            return self.app_record(app.id).await;
        }
        let made = self.github.make_app(&code).await?;
        let said = GithubAppSaid { slug: made.slug.clone(), name: made.name.clone(), owner: made.owner.clone() };
        let (logins, made_at_ms) = (Arc::clone(&self.github_logins), now_ms());
        let AppChange { app, changed } = off_thread(move || logins.keep_app(&made, &code)).await?;
        if changed {
            // What GitHub said of it as it made it, as it would say asked as the app.
            self.observation_read::<Said>(app.id, said, made_at_ms).await;
            self.notify(HubNotice::GithubApp(app.clone()));
        }
        match self.look_at_app(app.id, true).await {
            Ok(looked) => Ok(looked),
            Err(e) => {
                warn!(
                    name: "github.app_not_looked_for",
                    app = %app.id,
                    error = e.message,
                    "Made the app, but couldn't look for where it's installed"
                );
                Ok(app)
            }
        }
    }

    /// Looks for where the app `id` is installed now, and connects each installation, as the person asked, as
    /// `GithubAppInstallationsConnect` says.
    pub(crate) async fn github_app_installations_connect(&self, id: GithubAppId) -> Result<GithubApp, ApiError> {
        self.look_at_app(id, true).await
    }

    /// Forgets the app `id`, as `GithubAppRemove` says, and tells every client of each of its installations' accounts
    /// that changed, then of the app's removal.
    pub(crate) async fn github_app_remove(&self, id: GithubAppId) -> Result<GithubApp, ApiError> {
        let logins = Arc::clone(&self.github_logins);
        let AppRemoved { app, rev, accounts } = off_thread(move || logins.remove_app(id, now_ms())).await?;
        for account in accounts {
            self.announce_github_account(GithubAccountChange { account, changed: true, settings: None });
        }
        self.notify(HubNotice::Removed { record: RecordKey::GithubApp { id }, rev });
        self.app_observations_gone(id).await;
        Ok(app)
    }
}
