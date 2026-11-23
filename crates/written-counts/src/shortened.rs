/// A count where there's room only for a few figures, such as a workspace's lines, `37k`: whole below a thousand, then
/// in thousands, and from a million in millions, with a tenth below ten of either, such as `1.2k`. Cut rather than
/// rounded, so a count never reads as more than it is, and `999,999` reads as `999k` rather than `1000k`.
pub fn shortened(count: usize) -> String {
    for (unit, suffix) in [(1_000_000, "M"), (1_000, "k")] {
        if count < unit {
            continue;
        }
        let whole = count / unit;
        let tenths = count / (unit / 10) % 10;
        return if whole < 10 && tenths > 0 { format!("{whole}.{tenths}{suffix}") } else { format!("{whole}{suffix}") };
    }
    count.to_string()
}
