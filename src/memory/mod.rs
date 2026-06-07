pub mod changelog;
pub mod inject;
pub mod store;
pub mod tasks;

/// Generates a short hex ID from the current nanosecond clock.
pub(crate) fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:06x}", nanos & 0xFFFFFF)
}
