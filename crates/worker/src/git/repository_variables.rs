//! The variables that tell git which repository it's in, which the worker never hands on from its own environment.

use process::Environment;

/// What git reads from its environment about the repository it's in: where its directory, work tree, index, objects
/// and shallow and graft files are, and where git runs from within it. `git rev-parse --local-env-vars` lists them,
/// and they're the ones git itself takes out when it runs a command in another repository, such as a submodule's, in
/// `sanitize_repo_env`. Like git, not `GIT_CONFIG_PARAMETERS` or `GIT_CONFIG_COUNT`, which are the settings a command
/// was given, such as with `git -c`, and go on to one in another repository.
const OF_A_REPOSITORY: [&str; 13] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// `machine`, the environment of the machine the worker runs on, with none of [`OF_A_REPOSITORY`], whether a process
/// would inherit it or `machine` gives it. Each workspace is a repository of its own, so a variable of one the worker
/// was started in, as from inside a git hook or a shell with `GIT_DIR` exported, would point every workspace's git at
/// that one, and the git of every agent, script and terminal in it. *Unconfirmed:* taking them out of every process
/// the worker starts, by git's own list.
pub(crate) fn in_no_repository(machine: Environment) -> Environment {
    OF_A_REPOSITORY.into_iter().fold(machine, Environment::without)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::process::Command;

    use super::*;
    use crate::testing::given;

    /// The settings a command was given, which git hands on to one in another repository.
    const HANDED_ON: [&str; 2] = ["GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT"];

    /// So a git that reads another is caught here, and the list made to have it too.
    #[test]
    fn they_re_every_one_this_machine_s_git_says_is_of_a_repository_but_its_settings() {
        let dir = tempfile::tempdir().unwrap();

        let listed = process::testing::git(dir.path(), &["rev-parse", "--local-env-vars"]);

        let of_a_repository: Vec<&str> = listed.lines().filter(|name| !HANDED_ON.contains(name)).collect();
        let missing: Vec<&&str> = of_a_repository.iter().filter(|name| !OF_A_REPOSITORY.contains(name)).collect();
        assert!(missing.is_empty(), "git reads these as of a repository, which the worker hands on: {missing:?}");
        assert!(of_a_repository.contains(&"GIT_DIR"), "git listed {listed:?}");
    }

    #[test]
    fn a_process_has_none_of_them_but_its_settings_whether_it_inherits_them_or_was_given_them() {
        let machine = given().with("GIT_DIR", "/elsewhere/.git").with("GIT_CONFIG_COUNT", "1");
        let mut git = Command::new("git");
        git.env("GIT_INDEX_FILE", "/elsewhere/.git/index");

        in_no_repository(machine).apply(&mut git);

        let given = |name: &str| git.get_envs().find(|(set, _)| *set == OsStr::new(name)).map(|(_, value)| value);
        for name in OF_A_REPOSITORY {
            assert_eq!(given(name), Some(None), "{name} is taken out");
        }
        assert_eq!(given("GIT_CONFIG_COUNT"), Some(Some(OsStr::new("1"))));
    }
}
