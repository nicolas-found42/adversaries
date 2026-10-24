use std::collections::HashMap;
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, KeepAliveStream, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use council_ui::model::BackendEvent;
use council_ui::session_loader::{load_session, scan_sessions};
use council_ui::transport::{RunConfig, engine_command};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Sender, channel};
use tokio::task::spawn_blocking;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::services::ServeDir;

#[derive(Parser, Debug)]
#[command(
    name = "council-bridge",
    about = "HTTP + SSE sidecar for Adversarial Council"
)]
struct Cli {
    #[arg(long, default_value_t = 8787)]
    port: u16,
}

#[derive(Clone, Default)]
struct AppState {
    child: Arc<Mutex<Option<Child>>>,
    run_id: Arc<Mutex<String>>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let state = AppState::default();
    let app = build_router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], cli.port));
    let Ok(listener) = tokio::net::TcpListener::bind(addr).await else {
        eprintln!("Failed to bind to {addr}");
        return;
    };
    println!("council-bridge listening on http://127.0.0.1:{}", cli.port);
    let _ = axum::serve(listener, app).await;
}

fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/run", post(run_handler))
        .route("/models", get(models_handler))
        .route("/context", post(context_handler))
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024))
        .route("/cancel", post(cancel_handler))
        .route("/sessions", get(sessions_handler))
        .route("/session", get(session_handler));
    Router::new()
        .merge(api.clone())
        .nest("/api", api)
        .fallback_service(ServeDir::new("dist").append_index_html_on_directories(true))
        .with_state(state)
}

async fn run_handler(State(state): State<AppState>, body: Bytes) -> Response {
    let Ok(config) = serde_json::from_slice::<RunConfig>(&body) else {
        return (StatusCode::BAD_REQUEST, "invalid run configuration").into_response();
    };
    let mut cmd = build_command(&config);
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn process").into_response();
    };
    let Some(stdout) = child.stdout.take() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to capture stdout",
        )
            .into_response();
    };
    start_run(&state, child, stdout, &config.request_id)
        .await
        .into_response()
}

fn build_command(config: &RunConfig) -> Command {
    let (program, args) = engine_command(config);
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.current_dir(env::var("COUNCIL_ROOT").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into()));
    cmd
}

async fn start_run(
    state: &AppState,
    child: Child,
    stdout: tokio::process::ChildStdout,
    run_id: &str,
) -> Sse<KeepAliveStream<AutoKillStream<ReceiverStream<Result<Event, Infallible>>>>> {
    let expected_pid = child.id();
    replace_active_child(state, child, run_id).await;
    let (tx, rx) = channel(128);
    let child_ref = Arc::clone(&state.child);
    tokio::spawn(pump_and_reap(stdout, tx, child_ref, expected_pid));
    let guarded = AutoKillStream {
        inner: ReceiverStream::new(rx),
        child: Arc::clone(&state.child),
        expected_pid,
    };
    Sse::new(guarded).keep_alive(KeepAlive::default().interval(Duration::from_secs(15)))
}

async fn replace_active_child(state: &AppState, new_child: Child, run_id: &str) {
    let mut lock = state.child.lock().await;
    *state.run_id.lock().await = run_id.into();
    kill_child_slot(&mut lock, "replacement").await;
    council_ui::process_diagnostics::record("bridge_spawned", new_child.id(), run_id);
    *lock = Some(new_child);
}

async fn cancel_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let mut lock = state.child.lock().await;
    let active_id = state.run_id.lock().await;
    let matches = params.get("run_id").is_none_or(|id| *id == *active_id);
    let cancelled = matches && kill_child_slot(&mut lock, "user_cancel").await;
    Json(json!({ "cancelled": cancelled }))
}

async fn kill_child_slot(slot: &mut Option<Child>, reason: &str) -> bool {
    let Some(mut child) = slot.take() else {
        return false;
    };
    let pid = child.id();
    if let Ok(Some(status)) = child.try_wait() {
        council_ui::process_diagnostics::record("bridge_exited", pid, &status.to_string());
    } else {
        council_ui::process_diagnostics::record("bridge_stop_requested", pid, reason);
        let _ = child.kill().await;
        let status = child.wait().await;
        council_ui::process_diagnostics::record("bridge_exited", pid, &format!("{status:?}"));
    }
    true
}

async fn pump_and_reap(
    stdout: tokio::process::ChildStdout,
    tx: Sender<Result<Event, Infallible>>,
    child_lock: Arc<Mutex<Option<Child>>>,
    expected_pid: Option<u32>,
) {
    pump_stdout_lines(stdout, tx).await;
    let mut lock = child_lock.lock().await;
    if lock.as_ref().and_then(Child::id) == expected_pid {
        let _ = kill_child_slot(&mut lock, "stream_ended").await;
    }
}

async fn pump_stdout_lines(
    stdout: tokio::process::ChildStdout,
    tx: Sender<Result<Event, Infallible>>,
) {
    let mut terminal = false;
    let mut reader = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        if let Ok(event) = serde_json::from_str::<BackendEvent>(&line) {
            terminal |= matches!(
                event,
                BackendEvent::Done { .. } | BackendEvent::Error { .. }
            );
            let event = Event::default().data(line);
            if tx.send(Ok(event)).await.is_err() {
                break;
            }
        }
    }
    if !terminal {
        let message = json!({"type":"error", "message":"Backend exited before completing this stage. Completed arguments are preserved."});
        let _ = tx
            .send(Ok(Event::default().data(message.to_string())))
            .await;
    }
}

struct AutoKillStream<S> {
    inner: S,
    child: Arc<Mutex<Option<Child>>>,
    expected_pid: Option<u32>,
}

impl<S: Stream + Unpin> Stream for AutoKillStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl<S> Drop for AutoKillStream<S> {
    fn drop(&mut self) {
        let slot = Arc::clone(&self.child);
        let expected_pid = self.expected_pid;
        tokio::spawn(async move {
            let mut lock = slot.lock().await;
            if lock.as_ref().and_then(Child::id) == expected_pid {
                let _ = kill_child_slot(&mut lock, "client_disconnected").await;
            }
        });
    }
}

async fn sessions_handler() -> Response {
    let res = spawn_blocking(|| scan_sessions(Path::new("reports"))).await;
    match res {
        Ok(list) => Json(list).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "failed to scan sessions").into_response(),
    }
}

async fn session_handler(Query(params): Query<HashMap<String, String>>) -> Response {
    let Some(raw_path) = params.get("path") else {
        return (StatusCode::BAD_REQUEST, "missing path parameter").into_response();
    };
    let Some(safe_path) = validate_reports_path(raw_path) else {
        return (StatusCode::NOT_FOUND, "session not found").into_response();
    };
    let path_str = safe_path.to_string_lossy().to_string();
    let res = spawn_blocking(move || load_session(&path_str)).await;
    match res {
        Ok(Some(detail)) => Json(detail).into_response(),
        _ => (StatusCode::NOT_FOUND, "session not found").into_response(),
    }
}

fn validate_reports_path(rel_path: &str) -> Option<PathBuf> {
    let reports_dir = Path::new("reports").canonicalize().ok()?;
    let candidate = Path::new(rel_path);
    let resolved = if candidate.is_relative() {
        if let Ok(canonical) = candidate.canonicalize() {
            canonical
        } else {
            reports_dir.join(candidate)
        }
    } else {
        candidate.to_path_buf()
    };
    let canonical = resolved.canonicalize().ok()?;
    if canonical.starts_with(&reports_dir) {
        Some(canonical)
    } else {
        None
    }
}

async fn models_handler() -> Response {
    match spawn_blocking(council_ui::roles::load_catalog).await {
        Ok(Ok(catalog)) => Json(catalog).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Could not load models").into_response(),
    }
}

async fn context_handler(Query(params): Query<HashMap<String, String>>, body: Bytes) -> Response {
    let name = params.get("name").map_or("context.txt", String::as_str);
    let safe_name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join(".council-context");
    if std::fs::create_dir_all(&folder).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let path = folder.join(format!("{stamp}_{safe_name}"));
    match std::fs::write(&path, body) {
        Ok(()) => path.to_string_lossy().into_owned().into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delayed_cancel_cannot_kill_a_replacement_run() {
        let state = AppState::default();
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        replace_active_child(&state, child, "new-run").await;
        let old = HashMap::from([("run_id".into(), "old-run".into())]);
        let response = cancel_handler(State(state.clone()), Query(old)).await;
        assert_eq!(response.0["cancelled"], false);
        assert!(state.child.lock().await.is_some());
        let current = HashMap::from([("run_id".into(), "new-run".into())]);
        let response = cancel_handler(State(state.clone()), Query(current)).await;
        assert_eq!(response.0["cancelled"], true);
        assert!(state.child.lock().await.is_none());
    }
}
