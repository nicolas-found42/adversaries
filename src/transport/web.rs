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

pub fn spawn(config: RunConfig) -> Result<BackendHandle, String> {
    let (tx, rx) = channel();
    let cancelled = Arc::new(AtomicBool::new(false));

    let request_id = config.request_id.clone();
    let body = serde_json::to_vec(&config).map_err(|e| format!("Cannot encode run: {e}"))?;
    let request = crate::transport::post_json("/api/run", body);

    let buffer = Mutex::new(Vec::new());
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
                crate::transport::post_json(format!("/api/cancel?run_id={request_id}"), Vec::new());
            ehttp::fetch(request, |_| {});
        }),
    ))
}

fn stream_callback(
    result: ehttp::Result<Part>,
    buffer: &Mutex<Vec<u8>>,
    tx: &std::sync::mpsc::Sender<BackendEvent>,
    cancelled: &AtomicBool,
) -> ControlFlow<()> {
    match result {
        Ok(Part::Response(response)) if !response.ok => {
            let _ = tx.send(BackendEvent::Error {
                message: if response.status == 409 { "This execution request was already submitted. Read saved work or review a new run.".into() } else { format!("The Council server could not start this plan (HTTP {}). Review inputs or retry the connection.", response.status) },
            });
            ControlFlow::Break(())
        }
        Ok(Part::Response(_)) => ControlFlow::Continue(()),
        Ok(Part::Chunk(bytes)) => {
            absorb_chunk(buffer, tx, &bytes);
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
fn absorb_chunk(buffer: &Mutex<Vec<u8>>, tx: &std::sync::mpsc::Sender<BackendEvent>, chunk: &[u8]) {
    let mut lines = Vec::new();
    if let Ok(mut buf) = buffer.lock() {
        buf.extend_from_slice(chunk);
        while let Some(pos) = buf.iter().position(|byte| *byte == b'\n') {
            lines.push(buf.drain(..=pos).collect::<Vec<_>>());
        }
    }
    for line in lines {
        let Ok(line) = String::from_utf8(line) else {
            let _ = tx.send(BackendEvent::Error {
                message: "The run stream contained invalid text. Completed stages remain readable."
                    .into(),
            });
            continue;
        };
        if let Some(data) = line.strip_prefix("data: ")
            && let Ok(event) = serde_json::from_str::<BackendEvent>(data.trim())
        {
            let _ = tx.send(event);
        }
    }
}

/// Async session listing for the browser: `GET /api/sessions`.
pub fn fetch_sessions_async() -> Receiver<Result<Vec<SessionSummary>, String>> {
    let (tx, rx) = channel();
    ehttp::fetch(ehttp::Request::get("/api/sessions"), move |result| {
        let sessions = result
            .map_err(|_| {
                "Could not reach saved decisions. Check the Council server, then refresh."
                    .to_owned()
            })
            .and_then(|r| {
                if !r.ok {
                    return Err(format!(
                        "Could not list saved decisions (HTTP {}).",
                        r.status
                    ));
                }
                serde_json::from_slice(&r.bytes)
                    .map_err(|_| "The saved-decision list could not be read.".to_owned())
            });
        let _ = tx.send(sessions);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_survives_arbitrary_sse_chunk_boundaries() {
        let buffer = Mutex::new(Vec::new());
        let (tx, rx) = channel();
        let event = b"data: {\"type\":\"error\",\"message\":\"\xE2\x82\xAC50 Nairobi\"}\n\n";
        for byte in event {
            absorb_chunk(&buffer, &tx, &[*byte]);
        }
        let BackendEvent::Error { message } = rx.try_recv().unwrap() else {
            panic!("Expected event");
        };
        assert_eq!(message, "€50 Nairobi");
    }
}
