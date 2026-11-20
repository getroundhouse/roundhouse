//! How `MediaRemote` names the Now Playing app: by the ID of its process.

/// An app, by the ID of its process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct App(i32);

impl App {
    /// The app `MediaRemote` names with `pid`, which is 0 when it names none.
    pub(crate) fn from_pid(pid: i32) -> Option<Self> {
        (pid > 0).then_some(Self(pid))
    }
}

#[cfg(test)]
mod tests {
    use super::App;

    #[test]
    fn no_process_is_no_app() {
        assert_eq!(App::from_pid(0), None);
        assert_eq!(App::from_pid(501), Some(App(501)));
    }
}
