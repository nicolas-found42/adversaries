//! Content-free transport lifecycle records survive engine crashes and cancellation.
use std::io::Write;

pub fn record(event: &str, pid: Option<u32>, detail: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("reports");
    if !ensure_reports(&root) {
        return;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut file) = options.open(root.join("transport.events.jsonl")) {
        let data = serde_json::json!({"schema_version": 1, "time": chrono::Utc::now().to_rfc3339(), "event": event, "pid": pid, "detail": detail});
        let _ = writeln!(file, "{data}");
        let _ = file.sync_data();
    }
}

/// A fresh install gets an owner-only directory; an existing one keeps the mode
/// the user chose. The engine writes its own artifacts beside these records.
fn ensure_reports(root: &std::path::Path) -> bool {
    #[cfg(unix)]
    let created = {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
    };
    #[cfg(not(unix))]
    let created = std::fs::create_dir_all(root);
    match created {
        Ok(()) => true,
        Err(error) => error.kind() == std::io::ErrorKind::AlreadyExists,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn record_survives_an_existing_reports_directory() {
        let marker = format!("synthetic-lifecycle-{}", std::process::id());
        super::record(&marker, None, "fixture");
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("reports/transport.events.jsonl");
        let text = std::fs::read_to_string(path).expect("lifecycle record must stay writable");
        assert!(
            text.contains(&marker),
            "existing reports directory must accept records"
        );
    }
}
