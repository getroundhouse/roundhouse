//! The calls about apps of your own, GitHub Apps the person makes through GitHub's manifest flow, whose installations
//! agents act as: asking for an app's manifest, making it from the code GitHub sends the person back with, connecting
//! where it's installed, and removing it.

use protocol::client::github_apps::{self, GithubAppManifest, GithubManifestCode};
use protocol::model::{GithubApp, GithubAppId};

use super::{Client, Result};

impl Client {
    /// Asks the hub for the manifest of an app of your own called `name`, which sends the person back to
    /// `redirect_url` once they've made it on GitHub, for the client to send GitHub as it is. See
    /// [`github_apps::GithubAppManifestWrite`].
    pub fn github_app_manifest_write(&self, name: &str, redirect_url: &str) -> Result<GithubAppManifest> {
        self.call(github_apps::GithubAppManifestWrite {
            name: name.to_string(),
            redirect_url: redirect_url.to_string(),
        })
    }

    /// Gives the hub `code`, which GitHub sent the person back with once they made an app, and answers with the app's
    /// record. See [`github_apps::GithubAppMake`].
    pub fn github_app_make(&self, code: GithubManifestCode) -> Result<GithubApp> {
        self.call(github_apps::GithubAppMake { code })
    }

    /// Has the hub look for where the app `id` is installed now, and connect it there, and answers with the app's
    /// record. See [`github_apps::GithubAppInstallationsConnect`].
    pub fn github_app_installations_connect(&self, id: GithubAppId) -> Result<GithubApp> {
        self.call(github_apps::GithubAppInstallationsConnect { id })
    }

    /// Has the hub forget the app `id`, its key and each installation's login, and answers with its record as it last
    /// was, which the feed then says has been removed. See [`github_apps::GithubAppRemove`].
    pub fn github_app_remove(&self, id: GithubAppId) -> Result<GithubApp> {
        self.call(github_apps::GithubAppRemove { id })
    }
}
