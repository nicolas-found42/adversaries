#![allow(dead_code)]
//! Browser transport: talks to the `council-bridge` sidecar over HTTP.
//!
//! Runs are an SSE stream (`POST /api/run`); the streaming `ehttp` client
//! delivers raw chunks that we re-buffer into `data: <json>` lines feeding
//! the same channel the native transport uses.
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use ehttp::streaming::Part;

use crate::model::{BackendEvent, SessionDetail, SessionSummary};
use crate::transport::{BackendHandle, RunConfig};

pub fn spawn(mut config: RunConfig) -> Result<BackendHandle, String> {
    let (tx, rx) = channel();
    let cancelled = Arc::new(AtomicBool::new(false));

    config.request_id = format!("{}", chrono::Utc::now().timestamp_micros());
    let request_id = config.request_id.clone();
    let body = serde_json::to_vec(&config).map_err(|e| format!("Cannot encode run: {e}"))?;
    let request = ehttp::Request::post("/api/run", body);

    let buffer = Mutex::new(String::new());
    let tx_for_stream = tx.clone();
    let cancelled_for_stream = Arc::clone(&cancelled);

    ehttp::streaming::fetch(request, move |result| {
        stream_callback(result, &buffer, &tx_for_stream, &cancelled_for_stream)
    });

    let cancelled_for_cancel = Arc::clone(&cancelled);
    Ok(BackendHandle::new(
        rx,
        Arc::new(move || {
            cancelled_for_cancel.store(true, Ordering::SeqCst);
            let request =
                ehttp::Request::post(format!("/api/cancel?run_id={request_id}"), Vec::new());
            ehttp::fetch(request, |_| {});
        }),
    ))
}

fn stream_callback(
    result: ehttp::Result<Part>,
    buffer: &Mutex<String>,
    tx: &std::sync::mpsc::Sender<BackendEvent>,
    cancelled: &AtomicBool,
) -> ControlFlow<()> {
    match result {
        Ok(Part::Response(response)) if !response.ok => {
            let _ = tx.send(BackendEvent::Error {
                message: format!("Bridge error: HTTP {}", response.status),
            });
            ControlFlow::Break(())
        }
        Ok(Part::Response(_)) => ControlFlow::Continue(()),
        Ok(Part::Chunk(bytes)) => {
            absorb_chunk(buffer, tx, &String::from_utf8_lossy(&bytes));
            if cancelled.load(Ordering::SeqCst) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
        Err(e) => {
            let _ = tx.send(BackendEvent::Error {
                message: format!("Bridge unreachable: {e}"),
            });
            ControlFlow::Break(())
        }
    }
}

/// Re-buffers raw stream chunks into complete `data:` lines, then parses
/// each line's JSON payload into a `BackendEvent`.
fn absorb_chunk(buffer: &Mutex<String>, tx: &std::sync::mpsc::Sender<BackendEvent>, chunk: &str) {
    let mut lines: Vec<String> = Vec::new();
    if let Ok(mut buf) = buffer.lock() {
        buf.push_str(chunk);
        while let Some(pos) = buf.find('\n') {
            let line: String = buf.drain(..=pos).collect();
            lines.push(line.trim_end().to_string());
        }
    }
    for line in lines {
        if let Some(data) = line.strip_prefix("data: ")
            && let Ok(event) = serde_json::from_str::<BackendEvent>(data.trim())
        {
            let _ = tx.send(event);
        }
    }
}

/// Async session listing for the browser: `GET /api/sessions`.
pub fn fetch_sessions_async() -> Receiver<Vec<SessionSummary>> {
    let (tx, rx) = channel();
    ehttp::fetch(ehttp::Request::get("/api/sessions"), move |result| {
        let sessions = result
            .ok()
            .filter(|r| r.ok)
            .and_then(|r| serde_json::from_slice::<Vec<SessionSummary>>(&r.bytes).ok());
        let _ = tx.send(sessions.unwrap_or_default());
    });
    rx
}

/// Async session detail for the browser: `GET /api/session?path=...`.
pub fn fetch_session_async(path: String) -> Receiver<Option<SessionDetail>> {
    let (tx, rx) = channel();
    let url = format!("/api/session?path={}", urlencoding_lite(&path));
    ehttp::fetch(ehttp::Request::get(url), move |result| {
        let detail = result
            .ok()
            .filter(|r| r.ok)
            .and_then(|r| serde_json::from_slice::<SessionDetail>(&r.bytes).ok());
        let _ = tx.send(detail);
    });
    rx
}

/// Percent-encodes the characters a URI query cannot carry verbatim.
fn urlencoding_lite(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}
