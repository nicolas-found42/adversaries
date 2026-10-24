//! Content-free transport lifecycle records survive engine crashes and cancellation.
use std::io::Write;

pub fn record(event: &str, pid: Option<u32>, detail: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("reports");
    if std::fs::create_dir_all(&root).is_err() {
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
