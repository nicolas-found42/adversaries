//! Native transport: spawns `uv run council.py --json-stream` as a child
//! process and streams its stdout into the shared event channel.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::model::BackendEvent;
use crate::transport::{BackendHandle, RunConfig, engine_command};

pub fn spawn(config: RunConfig) -> Result<BackendHandle, String> {
    let (tx, rx) = channel();
    let cancelled = Arc::new(AtomicBool::new(false));

    let mut child = build_command(&config)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch council.py: {e}"))?;

    crate::process_diagnostics::record("native_spawned", Some(child.id()), &config.request_id);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout of council.py".to_string())?;

    let pid = child.id();
    let holder = Arc::new(ChildHolder::new(child));
    let holder_for_cancel = Arc::clone(&holder);
    let cancelled_for_reader = Arc::clone(&cancelled);
    let cancelled_for_cancel = Arc::clone(&cancelled);

    thread::spawn(move || {
        stream_stdout(stdout, tx, cancelled_for_reader, pid);
        holder.reap();
    });

    Ok(BackendHandle::new(
        rx,
        Arc::new(move || {
            cancelled_for_cancel.store(true, Ordering::SeqCst);
            holder_for_cancel.kill();
        }),
    ))
}

fn build_command(config: &RunConfig) -> Command {
    let (program, args) = engine_command(config);
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
    cmd
}

/// Parses each stdout line into a `BackendEvent`; unparseable lines are
/// dropped so chatty stderr leakage piped to stdout can never crash the UI.
fn stream_stdout(
    stdout: std::process::ChildStdout,
    tx: std::sync::mpsc::Sender<BackendEvent>,
    cancelled: Arc<AtomicBool>,
    pid: u32,
) {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        match line {
            Ok(text) => forward_event(&tx, &text, pid),
            Err(e) => {
                let _ = tx.send(BackendEvent::Error {
                    message: format!("IO error reading process output: {e}"),
                });
                break;
            }
        }
    }
}

fn forward_event(tx: &std::sync::mpsc::Sender<BackendEvent>, text: &str, pid: u32) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(event) = serde_json::from_str::<BackendEvent>(trimmed) {
        let _ = tx.send(event);
    } else {
        crate::process_diagnostics::record(
            "native_invalid_event",
            Some(pid),
            "invalid engine JSON; content omitted",
        );
    }
}

/// Keeps the child killable after `spawn` returns without blocking the UI.
struct ChildHolder {
    inner: Mutex<Option<std::process::Child>>,
}

impl ChildHolder {
    fn new(child: std::process::Child) -> Self {
        Self {
            inner: Mutex::new(Some(child)),
        }
    }

    fn reap(&self) {
        if let Ok(mut lock) = self.inner.lock()
            && let Some(mut child) = lock.take()
        {
            let status = child.wait();
            crate::process_diagnostics::record(
                "native_exited",
                Some(child.id()),
                &format!("{status:?}"),
            );
        }
    }

    fn kill(&self) {
        if let Ok(mut lock) = self.inner.lock()
            && let Some(mut child) = lock.take()
        {
            crate::process_diagnostics::record(
                "native_cancelled",
                Some(child.id()),
                "user or handle cancelled",
            );
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
