use super::{PathBuf, Stack};

/// Who it says it's logged in to github.com as, for `gh auth status`, in the form `gh` 2.103 prints, with only what's
/// read of it.
pub(crate) const LOGGED_IN_AS: &str = "ada";
/// The token it says it uses for that login, for `gh auth token`.
pub(crate) const LOGGED_IN_WITH: &str = "gho_made_up_for_ada";

/// A `gh` at the front of the hub's `PATH`, which says it's logged in as [`LOGGED_IN_AS`], with the token
/// [`LOGGED_IN_WITH`], so nothing an agent or the hub asks `gh` reaches GitHub. Other commands fail. And an `ssh`
/// beside it, which answers as GitHub does a key it takes as [`LOGGED_IN_AS`]'s, so nothing the hub asks `ssh` leaves
/// the machine.
pub(crate) struct StandInGh {
    bin: PathBuf,
}

impl StandInGh {
    /// Replaces the stack's hub with one that uses it.
    pub(crate) fn start(stack: &mut Stack) -> Self {
        stack.kill_hub();
        let gh = Self { bin: stack.path("bin") };
        std::fs::create_dir_all(&gh.bin).unwrap();
        process::testing::write_program(&gh.bin.join("gh"), Self::script()).unwrap();
        process::testing::write_program(&gh.bin.join("ssh"), Self::ssh_script()).unwrap();
        // Workers get the hub's PATH, since the stack's hubs have no login shell to ask for one.
        stack.env.put_first_on_path(&gh.bin);
        stack.start_hub();
        gh
    }

    /// The stand-in for `ssh`, as a shell script.
    fn ssh_script() -> String {
        format!(
            "#!/bin/sh\necho \"Hi {LOGGED_IN_AS}! You've successfully authenticated, but GitHub does not provide shell \
             access.\" >&2\nexit 1\n"
        )
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
