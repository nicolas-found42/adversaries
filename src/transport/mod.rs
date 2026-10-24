//! Pluggable debate-transport seam.
//!
//! Every transport funnels `BackendEvent`s into a `std::sync::mpsc` channel that
//! `App::tick` drains once per frame. Native runs spawn `council.py` directly;
//! browser builds talk to the `council-bridge` sidecar over HTTP + SSE.

pub mod mock;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod web;

#[cfg(target_arch = "wasm32")]
pub use web::{fetch_session_async, fetch_sessions_async};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::model::BackendEvent;

/// Parameters for one council debate run, shared by every transport.
///
/// The serde shape is the wire format of the bridge's `POST /api/run` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunConfig {
    pub question: String,
    #[serde(default)]
    pub request_id: String,
    pub rounds: u32,
    pub dry_run: bool,
    pub providers: Vec<String>,
    pub context_files: Vec<String>,
    #[serde(default)]
    pub roles: std::collections::BTreeMap<String, crate::roles::Selection>,
    #[serde(default)]
    pub completed_turns: Vec<crate::model::TurnState>,
    #[serde(default = "default_attempts")]
    pub max_attempts: u32,
}

fn default_attempts() -> u32 {
    1
}

/// CLI arguments for `uv run council.py --json-stream`.
///
/// The native transport and the `council-bridge` sidecar both build the
/// command line through this function so they can never drift apart.
pub fn council_args(config: &RunConfig) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "council.py".to_string(),
        "--json-stream".to_string(),
    ];
    if config.dry_run {
        args.push("--dry-run".to_string());
    }
    if config.rounds > 1 {
        args.push("--rounds".to_string());
        args.push(config.rounds.to_string());
    }
    if !config.providers.is_empty() {
        args.push("--providers".to_string());
        args.push(config.providers.join(","));
    }
    for file in &config.context_files {
        args.push("--context".to_string());
        args.push(file.clone());
    }
    if !config.roles.is_empty() {
        args.extend([
            "--roles-json".into(),
            serde_json::to_string(&config.roles).expect("role serialization"),
        ]);
    }
    if !config.completed_turns.is_empty() {
        args.extend([
            "--resume-json".into(),
            serde_json::to_string(&config.completed_turns).expect("turn serialization"),
        ]);
    }
    args.extend([
        "--max-attempts".into(),
        "1".into(),
        "--request-id".into(),
        config.request_id.clone(),
    ]);
    args.push("--".into());
    args.push(config.question.clone());
    args
}

#[cfg(not(target_arch = "wasm32"))]
pub fn engine_command(config: &RunConfig) -> (std::path::PathBuf, Vec<String>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let args = council_args(config);
    if python.exists() {
        (python, args.into_iter().skip(1).collect())
    } else {
        ("uv".into(), args)
    }
}

type CancelFn = Arc<dyn Fn() + Send + Sync>;
type PumpFn = Box<dyn FnMut() + Send>;

pub struct BackendHandle {
    pub receiver: Receiver<BackendEvent>,
    #[allow(dead_code)]
    cancel: CancelFn,
    pump: Option<Mutex<PumpFn>>,
}

impl BackendHandle {
    pub(crate) fn new(receiver: Receiver<BackendEvent>, cancel: CancelFn) -> Self {
        Self {
            receiver,
            cancel,
            pump: None,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn with_pump(
        receiver: Receiver<BackendEvent>,
        cancel: CancelFn,
        pump: Mutex<PumpFn>,
    ) -> Self {
        Self {
            receiver,
            cancel,
            pump: Some(pump),
        }
    }

    /// Stop the debate and release the engine process.
    pub fn cancel(&self) {
        (self.cancel)();
    }

    /// Called once per UI frame so callback-driven (browser) transports can
    /// push queued events into the receiver; native transports are no-ops.
    pub fn pump_frame(&self) {
        if let Some(pump) = &self.pump
            && let Ok(mut pump) = pump.lock()
        {
            pump();
        }
    }
}

/// Start a real council debate: native spawns `council.py`, browser calls the bridge.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn(config: RunConfig) -> Result<BackendHandle, String> {
    native::spawn(config)
}

/// Start a real council debate through the council-bridge sidecar.
#[cfg(target_arch = "wasm32")]
pub fn spawn(config: RunConfig) -> Result<BackendHandle, String> {
    web::spawn(config)
}

/// Start the built-in simulated debate (offline demo, zero API calls).
pub fn spawn_mock(question: String, fast: bool) -> BackendHandle {
    let (tx, rx) = channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut stream = mock::MockStream::new(question, fast);
    let cancelled_for_cancel = Arc::clone(&cancelled);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let handle_cancelled = Arc::clone(&cancelled_for_cancel);
        std::thread::spawn(move || mock::drive_in_thread(&mut stream, &tx, &handle_cancelled));
        BackendHandle::new(
            rx,
            Arc::new(move || cancelled_for_cancel.store(true, Ordering::SeqCst)),
        )
    }
    #[cfg(target_arch = "wasm32")]
    {
        let cancelled_for_pump = Arc::clone(&cancelled_for_cancel);
        let pump_stream = Mutex::new(Box::new(move || {
            for event in mock::MockStream::poll(&mut stream, &cancelled_for_pump) {
                let _ = tx.send(event);
            }
        }) as PumpFn);
        BackendHandle::with_pump(
            rx,
            Arc::new(move || cancelled_for_cancel.store(true, Ordering::SeqCst)),
            pump_stream,
        )
    }
}
