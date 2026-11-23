use std::time::Duration;

/// How long a video is, or how far into it, as a player says it: minutes and seconds, as `0:07` or `12:30`, and hours
/// before them once there are any, as `1:02:03`, in whole seconds, so a video of 7.2 seconds is `0:07` long.
pub fn length(of: Duration) -> String {
    let seconds = of.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}
