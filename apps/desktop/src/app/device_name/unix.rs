use std::process::Command;

pub(crate) fn computer_name() -> String {
    if cfg!(target_os = "macos")
        && let Ok(output) = Command::new("scutil").args(["--get", "ComputerName"]).output()
        && output.status.success()
        && let Ok(text) = String::from_utf8(output.stdout)
    {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let host = host_name();
    host.split('.').next().unwrap_or_default().to_string()
}

fn host_name() -> String {
    let mut buffer = [0u8; 256];
    // SAFETY: gethostname writes at most the length it's given into buffer, which outlives the call.
    if unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } != 0 {
        return String::new();
    }
    let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}
