use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::env;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, KeepAliveStream, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use council_ui::model::BackendEvent;
use council_ui::session_loader::{load_session, scan_sessions_checked};
use council_ui::transport::{RunConfig, engine_json_command};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
    dispatch: Arc<Mutex<()>>,
    requests: Arc<Mutex<HashSet<String>>>,
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
        .route("/preflight", post(preflight_handler))
        .route("/models", get(models_handler))
        .route("/context", post(context_handler))
        .layer(axum::extract::DefaultBodyLimit::max(
            council_ui::attachments::MAX_UPLOAD_BYTES,
        ))
        .route("/cancel", post(cancel_handler))
        .route("/sessions", get(sessions_handler))
        .route("/session", get(session_handler))
        .layer(middleware::from_fn(guard_local_request));
    Router::new()
        .merge(api.clone())
        .nest("/api", api)
        .fallback_service(ServeDir::new("dist").append_index_html_on_directories(true))
        .with_state(state)
}

/// The bridge has no authentication, so it accepts only what a page served from
/// this machine can send: a loopback `Host`, a loopback `Origin`, and a JSON body.
async fn guard_local_request(request: Request, next: Next) -> Response {
    if request_allowed(request.method(), request.headers()) {
        return next.run(request).await;
    }
    (StatusCode::FORBIDDEN, FORBIDDEN_MESSAGE).into_response()
}

const FORBIDDEN_MESSAGE: &str = "Rejected a request that did not come from this Council app.";

fn request_allowed(method: &Method, headers: &HeaderMap) -> bool {
    let host = headers
        .get(header::HOST)
        .is_none_or(|value| header_matches(value, loopback_authority));
    let origin = headers
        .get(header::ORIGIN)
        .is_none_or(|value| header_matches(value, loopback_origin));
    let body = *method != Method::POST
        || headers
            .get(header::CONTENT_TYPE)
            .is_some_and(is_json_content_type);
    host && origin && body
}

fn header_matches(value: &HeaderValue, test: fn(&str) -> bool) -> bool {
    value.to_str().is_ok_and(test)
}

fn loopback_origin(origin: &str) -> bool {
    origin
        .strip_prefix("http://")
        .is_some_and(loopback_authority)
}

fn loopback_authority(authority: &str) -> bool {
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, port)| {
            if port.chars().all(|digit| digit.is_ascii_digit()) {
                host
            } else {
                authority
            }
        });
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

fn is_json_content_type(value: &HeaderValue) -> bool {
    value.to_str().is_ok_and(|text| {
        text.split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    })
}

async fn run_handler(State(state): State<AppState>, body: Bytes) -> Response {
    let Ok(config) = serde_json::from_slice::<RunConfig>(&body) else {
        return (StatusCode::BAD_REQUEST, "invalid run configuration").into_response();
    };
    let _dispatch = state.dispatch.lock().await;
    if !reviewed_http_plan(&config) {
        return (
            StatusCode::BAD_REQUEST,
            "Review the execution inputs before starting.",
        )
            .into_response();
    }
    if !state
        .requests
        .lock()
        .await
        .insert(config.request_id.clone())
    {
        return (
            StatusCode::CONFLICT,
            "This execution request was already submitted.",
        )
            .into_response();
    }
    launch_run(&state, &config, &body).await
}

fn reviewed_http_plan(config: &RunConfig) -> bool {
    !config.request_id.is_empty()
        && config.request_id.len() <= 200
        && (1..=5).contains(&config.rounds)
        && !config.question.trim().is_empty()
        && config.context_snapshot.is_some()
}

async fn launch_run(state: &AppState, config: &RunConfig, body: &[u8]) -> Response {
    let mut cmd = build_command();
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn process").into_response();
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(body).await.is_err() {
            let _ = child.kill().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not send execution plan",
            )
                .into_response();
        }
    }
    let Some(stdout) = child.stdout.take() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to capture stdout",
        )
            .into_response();
    };
    start_run(state, child, stdout, &config.request_id)
        .await
        .into_response()
}

fn build_command() -> Command {
    let (program, args) = engine_json_command(false);
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
    let res = spawn_blocking(|| scan_sessions_checked(Path::new("reports"))).await;
    match res {
        Ok(Ok(list)) => Json(list).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "failed to scan sessions").into_response(),
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

async fn preflight_handler(body: Bytes) -> Response {
    let Ok(config) = serde_json::from_slice::<RunConfig>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if config.context_snapshot.is_none()
        && !config.context_files.iter().all(|p| valid_context_path(p))
    {
        return (
            StatusCode::BAD_REQUEST,
            "Choose a document uploaded to this Council server.",
        )
            .into_response();
    }
    match spawn_blocking(move || council_ui::preflight::check_native(&config)).await {
        Ok(Ok(report)) => Json(report).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Input check failed").into_response(),
    }
}

fn valid_context_path(path: &str) -> bool {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".council-context");
    match (root.canonicalize(), Path::new(path).canonicalize()) {
        (Ok(root), Ok(file)) => file.starts_with(root) && file.is_file(),
        _ => false,
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
    if !council_ui::attachments::valid_document_name(name) {
        return (
            StatusCode::BAD_REQUEST,
            council_ui::attachments::INVALID_DOCUMENT_NAME,
        )
            .into_response();
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".council-context");
    match store_upload(&root, name, &body) {
        Ok(path) => path.to_string_lossy().into_owned().into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn store_upload(root: &Path, name: &str, body: &[u8]) -> std::io::Result<PathBuf> {
    if !council_ui::attachments::valid_document_name(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            council_ui::attachments::INVALID_DOCUMENT_NAME,
        ));
    }
    private_directory(root)?;
    let folder = root.join(uuid::Uuid::new_v4().to_string());
    private_directory(&folder)?;
    let path = folder.join(name);
    if let Err(error) = write_private(&path, body) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&folder);
        return Err(error);
    }
    Ok(path)
}

/// Uploads are executive material: the directory is owner-only and existing ones
/// are tightened too, because earlier builds created them world-readable.
fn private_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(path)?;
    tighten(path, 0o700)
}

fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(body)?;
    tighten(path, 0o600)
}

#[cfg(unix)]
fn tighten(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn tighten(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        path.metadata().unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn upload_storage_preserves_same_names_in_separate_directories() {
        let root = tempfile::tempdir().unwrap();
        let name = "Planning café & costs #1 + 100%.txt";
        let first = store_upload(root.path(), name, b"First source.").unwrap();
        let second = store_upload(root.path(), name, b"Second source.").unwrap();
        assert_eq!(first.file_name().unwrap(), name);
        assert_eq!(second.file_name().unwrap(), name);
        assert_ne!(first.parent(), second.parent());
        assert_eq!(first.parent().unwrap().parent(), Some(root.path()));
        assert_eq!(std::fs::read(&first).unwrap(), b"First source.");
        assert_eq!(std::fs::read(&second).unwrap(), b"Second source.");
        assert_eq!(mode(&first), 0o600, "uploads hold executive material");
        assert_eq!(mode(first.parent().unwrap()), 0o700);
        assert_eq!(mode(root.path()), 0o700);
        assert!(store_upload(root.path(), &"x".repeat(300), b"Never stored").is_err());
        assert_eq!(
            store_upload(root.path(), "C:escape.txt", b"Never stored")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            2,
            "failed writes must not leave empty upload directories"
        );
    }

    #[tokio::test]
    async fn upload_handler_rejects_path_names_before_creating_storage() {
        for name in [
            "../escape.txt",
            "C:escape.txt",
            r"folder\escape.pdf",
            ".",
            "..",
            "line\nbreak.txt",
        ] {
            let params = HashMap::from([("name".into(), name.into())]);
            let response = context_handler(Query(params), Bytes::from_static(b"Synthetic")).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn legacy_flat_uploads_remain_eligible_for_preflight() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".council-context");
        std::fs::create_dir_all(&root).unwrap();
        let legacy = tempfile::Builder::new()
            .prefix("1793_synthetic_legacy_")
            .suffix(".txt")
            .tempfile_in(root)
            .unwrap();
        std::fs::write(legacy.path(), "Synthetic legacy material.").unwrap();
        let config = RunConfig {
            question: "Synthetic legacy input check?".into(),
            rounds: 1,
            dry_run: true,
            context_files: vec![legacy.path().to_string_lossy().into_owned()],
            ..Default::default()
        };
        let response = preflight_handler(Bytes::from(serde_json::to_vec(&config).unwrap())).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let report: council_ui::preflight::PreflightReport = serde_json::from_slice(&body).unwrap();
        assert!(report.ready);
        assert_eq!(
            report.documents[0].snapshot.name,
            legacy.path().file_name().unwrap().to_str().unwrap()
        );
        assert_eq!(
            report.documents[0].snapshot.text,
            "Synthetic legacy material."
        );
    }

    #[tokio::test]
    async fn rebinding_and_cross_origin_requests_are_rejected() {
        let rebound = guarded(request("GET", "/api/models", &[("host", "evil.example")])).await;
        assert_eq!(rebound, StatusCode::FORBIDDEN);
        let cross_site = guarded(request(
            "POST",
            "/api/cancel",
            &[
                ("host", "127.0.0.1:8787"),
                ("origin", "https://evil.example"),
                ("content-type", "application/json"),
            ],
        ))
        .await;
        assert_eq!(cross_site, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_form_post_without_a_json_body_is_rejected() {
        let status = guarded(request(
            "POST",
            "/api/cancel",
            &[("host", "127.0.0.1:8787"), ("content-type", "text/plain")],
        ))
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn the_root_mounted_api_alias_is_guarded_too() {
        let status = guarded(request(
            "POST",
            "/cancel",
            &[("host", "127.0.0.1:8787"), ("content-type", "text/plain")],
        ))
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn app_requests_including_the_dev_proxy_reach_the_handler() {
        let status = guarded(request(
            "POST",
            "/api/cancel",
            &[
                ("host", "127.0.0.1:8787"),
                ("origin", "http://127.0.0.1:8080"),
                ("content-type", "application/json"),
            ],
        ))
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    fn request(
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> axum::http::Request<axum::body::Body> {
        let mut builder = axum::http::Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(axum::body::Body::empty()).unwrap()
    }

    async fn guarded(req: axum::http::Request<axum::body::Body>) -> StatusCode {
        use tower::ServiceExt;
        build_router(AppState::default())
            .oneshot(req)
            .await
            .unwrap()
            .status()
    }

    #[test]
    fn only_literal_loopback_authorities_are_accepted() {
        for allowed in [
            "127.0.0.1",
            "127.0.0.1:8787",
            "localhost:8080",
            "[::1]:8787",
        ] {
            assert!(loopback_authority(allowed), "{allowed} must be accepted");
        }
        for rejected in [
            "evil.example",
            "evil.example:8787",
            "127.0.0.1.nip.io:8787",
            "user@127.0.0.1",
        ] {
            assert!(!loopback_authority(rejected), "{rejected} must be rejected");
        }
        assert!(loopback_origin("http://127.0.0.1:8080"));
        assert!(!loopback_origin("https://127.0.0.1:8080"));
        assert!(!loopback_origin("null"));
    }

    #[tokio::test]
    async fn duplicate_dispatch_cannot_replace_the_original_process() {
        let state = AppState::default();
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        replace_active_child(&state, child, "approved-request").await;
        state
            .requests
            .lock()
            .await
            .insert("approved-request".into());
        let config = RunConfig {
            question: "Q".into(),
            rounds: 1,
            dry_run: true,
            request_id: "approved-request".into(),
            context_snapshot: Some(Vec::new()),
            ..Default::default()
        };
        let response = run_handler(
            State(state.clone()),
            Bytes::from(serde_json::to_vec(&config).unwrap()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(state.child.lock().await.as_ref().unwrap().id(), pid);
        kill_child_slot(&mut *state.child.lock().await, "test_cleanup").await;
    }

    #[tokio::test]
    async fn browser_preflight_rejects_unuploaded_server_files() {
        let config = RunConfig {
            question: "Q".into(),
            rounds: 1,
            dry_run: true,
            context_files: vec!["Cargo.toml".into()],
            ..Default::default()
        };
        let response = preflight_handler(Bytes::from(serde_json::to_vec(&config).unwrap())).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

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
