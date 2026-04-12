use std::time::Duration;

pub(crate) fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    if total_secs >= 60 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        if secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {}s", mins, secs)
        }
    } else {
        format!("{}s", total_secs)
    }
}
