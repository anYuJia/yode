use chrono::{Local, NaiveDateTime};

pub(crate) fn parse_runtime_timestamp(value: Option<&str>) -> Option<NaiveDateTime> {
    value.and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok())
}

pub(crate) fn memory_freshness_label(last_update_at: Option<&str>) -> &'static str {
    let Some(last_update) = parse_runtime_timestamp(last_update_at) else {
        return "unknown";
    };
    let age = Local::now().naive_local() - last_update;
    if age.num_minutes() <= 10 {
        "fresh"
    } else if age.num_minutes() <= 60 {
        "warm"
    } else {
        "stale"
    }
}
