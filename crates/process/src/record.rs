use std::io;
use std::path::Path;
use std::process::Command;

/// When a process started, as `ps` prints it, or `None` if it isn't running.
pub fn start_time(pid: u32) -> Option<String> {
    let output = Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let started = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!started.is_empty()).then_some(started)
}

/// A PID and its start time, kept in a file, because a PID alone could have been reused by the time
/// anyone reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRecord {
    pub pid: u32,
    pub started: String,
}

impl ProcessRecord {
    /// Records a running process.
    pub fn write(path: &Path, pid: u32) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{pid}\n{}\n", start_time(pid).unwrap_or_default()))
    }

    /// `None` if there's no usable record, including one that couldn't get a start time, since that
    /// can't prove which process it means.
    pub fn read(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        let mut lines = contents.lines();
        let pid = lines.next()?.parse().ok()?;
        let started = lines.next().filter(|line| !line.is_empty())?.to_string();
        Some(Self { pid, started })
    }

    /// Whether the recorded process is still the one running under this PID.
    pub fn is_running(&self) -> bool {
        start_time(self.pid).as_deref() == Some(self.started.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_identifies_a_running_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents/one");

        ProcessRecord::write(&path, std::process::id()).unwrap();

        let record = ProcessRecord::read(&path).unwrap();
        assert_eq!(record.pid, std::process::id());
        assert!(record.is_running());
    }

    #[test]
    fn a_reused_pid_is_not_the_recorded_process() {
        let record = ProcessRecord { pid: std::process::id(), started: "long ago".to_string() };

        assert!(!record.is_running());
    }

    #[test]
    fn a_record_without_a_start_time_is_not_usable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent");

        std::fs::write(&path, "123\n\n").unwrap();
        assert_eq!(ProcessRecord::read(&path), None);

        std::fs::write(&path, "not a pid\nsomeday\n").unwrap();
        assert_eq!(ProcessRecord::read(&path), None);

        assert_eq!(ProcessRecord::read(&dir.path().join("missing")), None);
    }
}
