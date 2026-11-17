use super::{PathBuf, Stack, install_executable};

/// Who it says it's logged in to github.com as, for `gh auth status`, in the form `gh` 2.103 prints, with only what's
/// read of it.
pub(crate) const LOGGED_IN_AS: &str = "ada";
/// The token it says it uses for that login, for `gh auth token`.
pub(crate) const LOGGED_IN_WITH: &str = "gho_made_up_for_ada";

/// A `gh` at the front of the hub's `PATH`, which says it's logged in as [`LOGGED_IN_AS`], with the token
/// [`LOGGED_IN_WITH`], as a machine's own `gh` hands the hub the token it reads pull requests with. Other commands fail.
pub(crate) struct StandInGh {
    bin: PathBuf,
}

impl StandInGh {
    /// Replaces the stack's hub with one that uses it.
    pub(crate) fn start(stack: &mut Stack) -> Self {
        stack.kill_hub();
        let gh = Self { bin: stack.path("bin") };
        std::fs::create_dir_all(&gh.bin).unwrap();
        let source = stack.path("gh.source");
        std::fs::write(&source, Self::script()).unwrap();
        install_executable(&source, &gh.bin.join("gh"));
        // Workers get the hub's PATH, since the stack's hubs have no login shell to ask for one.
        stack.env.put_first_on_path(&gh.bin);
        stack.start_hub();
        gh
    }

    /// The stand-in itself, as a shell script.
    fn script() -> String {
        format!(
            r#"#!/bin/sh
[ "$1 $2" = 'auth status' ] && exec echo '{{"hosts":{{"github.com":[{{"state":"success","active":true,"login":"{LOGGED_IN_AS}"}}]}}}}'
[ "$1 $2" = 'auth token' ] && exec echo '{LOGGED_IN_WITH}'
exit 1
"#
        )
    }
}
