use std::sync::mpsc::Receiver;

use egui::Key;

use crate::claims::extract_claims;
use crate::model::{
    BackendEvent, Claim, SessionDetail, SessionSummary, Tab, TurnState, TurnStatus,
};
use crate::transport::{BackendHandle, RunConfig, spawn, spawn_mock};
use crate::ui::composer::ComposerState;

#[cfg(not(target_arch = "wasm32"))]
use crate::session_loader::{load_session, scan_sessions};

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(target_arch = "wasm32")]
use crate::transport::{fetch_session_async, fetch_sessions_async};

#[cfg(not(target_arch = "wasm32"))]
const REPORTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/reports");

/// UI-framework-independent council state: owns the debate transcript, the
/// claim ledger, session history and the live backend handle.
pub struct App {
    pub active_tab: Tab,
    pub turns: Vec<TurnState>,
    pub selected_turn: usize,
    pub brief_text: String,
    pub claims: Vec<Claim>,
    pub selected_claim: usize,
    pub sessions: Vec<SessionSummary>,
    pub selected_session: usize,
    pub is_running: bool,
    pub status_message: String,
    pub current_question: String,
    pub session_key: String,
    pub composer_open: bool,
    pub composer_state: ComposerState,
    pub backend: Option<BackendHandle>,
    pub theme: crate::ui::theme::Theme,
    pub demo_mode: bool,
    pub should_quit: bool,
    pub follow_live: bool,
    pub inspector_open: bool,
    pub total_calls: usize,
    pub error: Option<String>,
    pub catalog: Vec<crate::roles::ProviderCatalog>,
    pub catalog_message: String,
    pub markdown_cache: egui_commonmark::CommonMarkCache,
    pub last_config: Option<RunConfig>,
    pub diagnostics: serde_json::Value,
    pub stage_started: Option<web_time::Instant>,
    pending_catalog: Option<Receiver<Result<Vec<crate::roles::ProviderCatalog>, String>>>,
    pending_sessions: Option<Receiver<Vec<SessionSummary>>>,
    pending_detail: Option<Receiver<Option<SessionDetail>>>,
}

impl App {
    pub fn new(demo_mode: bool, initial_question: Option<String>, dry_run: bool) -> Self {
        let sessions = Vec::new();
        let mut app = Self {
            active_tab: Tab::Debate,
            turns: Vec::new(),
            selected_turn: 0,
            brief_text: String::new(),
            claims: Vec::new(),
            selected_claim: 0,
            sessions,
            selected_session: 0,
            is_running: false,
            status_message: "Ready for your next decision.".to_string(),
            current_question: initial_question.clone().unwrap_or_default(),
            session_key: String::new(),
            composer_open: false,
            composer_state: ComposerState {
                question: initial_question.unwrap_or_default(),
                rounds: 1,
                dry_run,
                ..ComposerState::default()
            },
            backend: None,
            theme: crate::ui::theme::Theme::default(),
            demo_mode,
            should_quit: false,
            follow_live: true,
            inspector_open: true,
            total_calls: 6,
            error: None,
            catalog: Vec::new(),
            catalog_message: "Load provider catalogs to see available models.".into(),
            markdown_cache: egui_commonmark::CommonMarkCache::default(),
            last_config: None,
            stage_started: None,
            diagnostics: serde_json::Value::Null,
            pending_catalog: None,
            pending_sessions: None,
            pending_detail: None,
        };
        app.refresh_sessions();
        if demo_mode {
            let q = "Should we expand our engineering team in Nairobi or London?".to_string();
            app.start_mock_run(q);
        } else if !app.current_question.is_empty() {
            let q = app.current_question.clone();
            app.start_run(q, 1, dry_run);
        }
        app
    }

    /// Drives everything time-based: pumps browser transports, drains backend
    /// events, and collects async session responses. Called once per frame.
    pub fn tick(&mut self) {
        if let Some(backend) = &self.backend {
            backend.pump_frame();
        }
        for event in self.collect_pending_events() {
            self.handle_backend_event(event);
        }
        self.poll_catalog();
        self.poll_pending_sessions();
        self.poll_pending_detail();
    }

    fn collect_pending_events(&mut self) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        if let Some(backend) = &self.backend {
            loop {
                match backend.receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if self.is_running
                            && !events.iter().any(|e| {
                                matches!(e, BackendEvent::Done { .. } | BackendEvent::Error { .. })
                            })
                        {
                            events.push(BackendEvent::Error { message: "Backend exited before completing the debate. Completed stages are preserved.".into() });
                        }
                        break;
                    }
                }
            }
        }
        events
    }

    fn poll_pending_sessions(&mut self) {
        let Some(rx) = &self.pending_sessions else {
            return;
        };
        match rx.try_recv() {
            Ok(sessions) => {
                self.sessions = sessions;
                self.pending_sessions = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pending_sessions = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    fn poll_pending_detail(&mut self) {
        let Some(rx) = &self.pending_detail else {
            return;
        };
        match rx.try_recv() {
            Ok(Some(detail)) => {
                self.apply_session_detail(detail);
                self.pending_detail = None;
            }
            Ok(None) => self.pending_detail = None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pending_detail = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    fn handle_backend_event(&mut self, event: BackendEvent) {
        match event {
            BackendEvent::Start {
                question,
                providers,
                total_calls,
                ..
            } => {
                self.total_calls = total_calls;
                self.current_question = question;
                self.status_message = format!(
                    "Debating: {} providers, {} calls total",
                    providers.len(),
                    total_calls
                );
            }
            BackendEvent::TurnStart { index, model, role } => {
                self.handle_turn_start(index, model, role);
            }
            BackendEvent::BriefStart { index, model, role } => {
                self.handle_turn_start(index, model, role);
            }
            BackendEvent::TurnComplete {
                index,
                model,
                role,
                text,
                seconds,
                diagnostics,
            } => self.apply_completed_turn(TurnState {
                index,
                model,
                role,
                text,
                seconds,
                diagnostics,
                status: TurnStatus::Complete,
            }),
            BackendEvent::BriefComplete {
                index,
                model,
                role,
                text,
                seconds,
                diagnostics,
            } => self.handle_brief_complete(TurnState {
                index,
                model,
                role,
                text,
                seconds,
                diagnostics,
                status: TurnStatus::Complete,
            }),
            BackendEvent::TurnFailed { turn } => {
                self.error = Some(
                    turn.diagnostics["message"]
                        .as_str()
                        .unwrap_or("Stage incomplete")
                        .into(),
                );
                self.apply_completed_turn(turn);
            }
            BackendEvent::Diagnostics { diagnostics } => self.diagnostics = diagnostics,
            BackendEvent::Done { report_path, .. } => {
                self.session_key = session_key(&report_path);
                self.is_running = false;
                self.backend = None;
                self.status_message = "Decision saved. Your brief is ready.".into();
                self.claims = extract_claims(&self.turns);
                self.refresh_sessions();
            }
            BackendEvent::Error { message } => {
                self.is_running = false;
                self.backend = None;
                self.error = Some(message.clone());
                if let Some(turn) = self
                    .turns
                    .iter_mut()
                    .find(|t| t.status == TurnStatus::Running)
                {
                    turn.status = TurnStatus::Failed;
                }
                self.status_message = format!("Stopped: {message}");
                self.refresh_sessions();
            }
        }
    }

    fn handle_turn_start(&mut self, index: usize, model: String, role: String) {
        if let Some(existing) = self.turns.iter_mut().find(|t| t.index == index) {
            existing.status = TurnStatus::Running;
        } else {
            let mut turn = TurnState::new(index, model, role);
            turn.status = TurnStatus::Running;
            self.turns.push(turn);
            if self.follow_live {
                self.selected_turn = self.turns.len().saturating_sub(1);
            }
        }
        self.stage_started = Some(web_time::Instant::now());
        self.status_message = format!("Running stage {index} of {}", self.total_calls);
    }

    fn apply_completed_turn(&mut self, turn: TurnState) {
        if let Some(existing) = self.turns.iter_mut().find(|t| t.index == turn.index) {
            *existing = turn;
        } else {
            self.turns.push(turn);
        }
        self.claims = extract_claims(&self.turns);
    }

    fn handle_brief_complete(&mut self, turn: TurnState) {
        self.brief_text.clone_from(&turn.text);
        self.apply_completed_turn(turn);
        self.status_message = "Your decision brief is ready.".into();
    }

    /// Global keyboard shortcuts. Inert while the composer modal owns input.
    pub fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.composer_open || ctx.egui_wants_keyboard_input() {
            return;
        }
        ctx.input(|input| self.apply_shortcuts(input));
    }

    fn apply_shortcuts(&mut self, input: &egui::InputState) {
        self.apply_tab_shortcuts(input);
        self.apply_debate_shortcuts(input);
        if input.key_pressed(Key::ArrowUp) || input.key_pressed(Key::K) {
            self.navigate_up();
        }
        if input.key_pressed(Key::ArrowDown) || input.key_pressed(Key::J) {
            self.navigate_down();
        }
    }

    fn apply_tab_shortcuts(&mut self, input: &egui::InputState) {
        for (key, tab) in [
            (Key::Num1, Tab::Debate),
            (Key::Num2, Tab::Brief),
            (Key::Num3, Tab::Claims),
            (Key::Num4, Tab::Sessions),
            (Key::Num5, Tab::Help),
            (Key::Num6, Tab::Diagnostics),
        ] {
            if input.key_pressed(key) {
                self.active_tab = tab;
            }
        }
        if input.key_pressed(Key::Tab) {
            self.next_tab();
        }
    }

    fn apply_debate_shortcuts(&mut self, input: &egui::InputState) {
        if input.key_pressed(Key::Enter) && self.active_tab == Tab::Sessions {
            self.load_selected_session();
        }
        if input.key_pressed(Key::N) {
            self.composer_open = true;
        }
        if input.key_pressed(Key::C) {
            self.cancel_run();
        }
        if input.key_pressed(Key::R) {
            self.rerun_current();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if input.key_pressed(Key::Q) {
            self.should_quit = true;
        }
    }

    pub fn navigate_up(&mut self) {
        match self.active_tab {
            Tab::Debate => {
                self.follow_live = false;
                self.selected_turn = self.selected_turn.saturating_sub(1);
            }
            Tab::Claims => self.selected_claim = self.selected_claim.saturating_sub(1),
            Tab::Sessions => self.selected_session = self.selected_session.saturating_sub(1),
            _ => {}
        }
    }

    pub fn navigate_down(&mut self) {
        match self.active_tab {
            Tab::Debate => {
                self.follow_live = false;
                if self.selected_turn + 1 < self.turns.len() {
                    self.selected_turn += 1;
                }
            }
            Tab::Claims => {
                if self.selected_claim + 1 < self.claims.len() {
                    self.selected_claim += 1;
                }
            }
            Tab::Sessions => {
                if self.selected_session + 1 < self.sessions.len() {
                    self.selected_session += 1;
                }
            }
            _ => {}
        }
    }

    pub fn next_tab(&mut self) {
        let next_idx = (self.active_tab as usize + 1) % Tab::ALL.len();
        self.active_tab = Tab::ALL[next_idx];
    }

    pub fn submit_composer(&mut self) {
        let question = self.composer_state.question.trim().to_string();
        if question.is_empty() {
            return;
        }
        let rounds = self.composer_state.rounds;
        let dry_run = self.composer_state.dry_run;
        self.composer_open = false;
        self.start_run(question, rounds, dry_run);
    }

    pub fn start_run(&mut self, question: String, rounds: u32, dry_run: bool) {
        self.cancel_run();
        self.pending_detail = None;
        self.session_key.clear();
        self.turns.clear();
        self.diagnostics = serde_json::Value::Null;
        self.brief_text.clear();
        self.claims.clear();
        self.current_question = question.clone();
        self.active_tab = Tab::Debate;
        self.selected_turn = 0;

        if self.demo_mode {
            self.start_mock_run(question);
            return;
        }

        let config = RunConfig {
            question,
            rounds,
            dry_run,
            providers: Vec::new(),
            context_files: self.composer_state.context_files.clone(),
            roles: self.composer_state.roles.clone(),
            max_attempts: 1,
            ..RunConfig::default()
        };
        self.last_config = Some(config.clone());
        self.error = None;
        self.total_calls = 2 + rounds as usize * 4;

        match spawn(config) {
            Ok(handle) => {
                self.backend = Some(handle);
                self.is_running = true;
                self.status_message = "Council started... connecting to backend".to_string();
            }
            Err(e) => self.status_message = format!("Launch failed: {e}"),
        }
    }

    pub fn start_mock_run(&mut self, question: String) {
        self.is_running = true;
        self.status_message = "Demo mode: running simulated adversarial debate...".to_string();
        self.backend = Some(spawn_mock(question, false));
    }

    pub fn cancel_run(&mut self) {
        if let Some(backend) = self.backend.take() {
            backend.cancel();
            self.is_running = false;
            self.status_message = "Debate stopped. Completed arguments are saved.".to_string();
            for turn in &mut self.turns {
                if turn.status == TurnStatus::Running {
                    turn.status = TurnStatus::Failed;
                }
            }
            self.refresh_sessions();
        }
    }

    pub fn rerun_current(&mut self) {
        if let Some(config) = self.last_config.clone() {
            self.composer_state.roles.clone_from(&config.roles);
            self.composer_state
                .context_files
                .clone_from(&config.context_files);
            self.start_run(config.question, config.rounds, config.dry_run);
        } else if !self.current_question.is_empty() {
            self.composer_state
                .question
                .clone_from(&self.current_question);
            self.composer_open = true;
        }
    }

    pub fn retry_stage(&mut self) {
        let Some(mut config) = self.last_config.clone() else {
            return;
        };
        config.completed_turns = self
            .turns
            .iter()
            .filter(|t| t.status == TurnStatus::Complete)
            .cloned()
            .collect();
        self.turns.retain(|t| t.status == TurnStatus::Complete);
        match spawn(config) {
            Ok(handle) => {
                self.backend = Some(handle);
                self.is_running = true;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn refresh_catalog(&mut self) {
        self.pending_catalog = Some(crate::roles::fetch_catalog());
        self.catalog_message = "Loading provider catalogs…".into();
    }

    pub fn needs_poll(&self) -> bool {
        self.is_running
            || self.catalog_loading()
            || self.composer_open
            || self.pending_sessions.is_some()
            || self.pending_detail.is_some()
    }

    pub fn catalog_loading(&self) -> bool {
        self.pending_catalog.is_some()
    }

    fn poll_catalog(&mut self) {
        let Some(rx) = &self.pending_catalog else {
            return;
        };
        if let Ok(result) = rx.try_recv() {
            match result {
                Ok(catalog) => {
                    for selection in self.composer_state.roles.values_mut() {
                        if selection.model == crate::roles::default_model(&selection.provider)
                            && let Some(entry) =
                                catalog.iter().find(|c| c.provider == selection.provider)
                            && !entry.default_model.is_empty()
                        {
                            selection.model.clone_from(&entry.default_model);
                        }
                    }
                    self.catalog = catalog;
                    self.catalog_message =
                        "Provider catalogs loaded. Custom model IDs are also supported.".into();
                }
                Err(error) => self.catalog_message = error,
            }
            self.pending_catalog = None;
        }
    }

    pub fn elapsed(&self, turn: &TurnState) -> f64 {
        if turn.status == TurnStatus::Running {
            self.stage_started
                .map_or(0.0, |t| t.elapsed().as_secs_f64())
        } else {
            turn.seconds
        }
    }

    pub fn load_selected_session(&mut self) {
        if let Some(session) = self.sessions.get(self.selected_session) {
            let path = session.path_str.clone();
            self.load_session_by_path(&path);
        }
    }

    pub fn load_session_by_path(&mut self, path: &str) {
        if self.is_running {
            return;
        }
        self.cancel_run();
        self.error = None;
        self.last_config = None;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(detail) = load_session(path) {
            self.apply_session_detail(detail);
            self.session_key = session_key(path);
            self.status_message = "Saved decision opened.".into();
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.session_key = session_key(path);
            self.pending_detail = Some(fetch_session_async(path.to_string()));
            self.status_message = "Opening saved decision…".into();
        }
    }

    fn apply_session_detail(&mut self, detail: SessionDetail) {
        self.last_config = detail.run_config;
        self.diagnostics = detail.diagnostics;
        self.total_calls = self
            .last_config
            .as_ref()
            .map_or(detail.turns.len() + 1, |c| 2 + c.rounds as usize * 4);
        if let Some(config) = &self.last_config {
            self.composer_state.question.clone_from(&config.question);
            self.composer_state.roles.clone_from(&config.roles);
            self.composer_state.rounds = config.rounds;
            self.composer_state.dry_run = config.dry_run;
            self.composer_state
                .context_files
                .clone_from(&config.context_files);
            if detail.brief.is_empty() {
                self.error = Some("This saved decision is incomplete. Retry to continue from the last completed stage.".into());
            }
        }
        self.current_question = detail.question;
        self.brief_text = detail.brief;
        self.turns = detail.turns;
        self.claims = extract_claims(&self.turns);
        self.selected_turn = 0;
        self.active_tab = if self.brief_text.is_empty() {
            Tab::Debate
        } else {
            Tab::Brief
        };
    }

    pub fn refresh_sessions(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.sessions = scan_sessions(&reports_directory());
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.pending_sessions = Some(fetch_sessions_async());
        }
    }
}

fn session_key(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn reports_directory() -> PathBuf {
    std::env::var_os("COUNCIL_REPORTS_DIR")
        .map_or_else(|| PathBuf::from(REPORTS_DIR), PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc::channel};

    #[test]
    fn unexpected_backend_exit_stops_spinner_and_preserves_completed_work() {
        let mut app = App::new(false, None, true);
        app.handle_turn_start(1, "Mistral / small".into(), "Opening".into());
        app.turns[0].text = "C1: Saved argument".into();
        app.turns[0].status = TurnStatus::Complete;
        app.handle_turn_start(2, "OpenRouter / llama".into(), "Red team 1".into());
        let (tx, rx) = channel();
        drop(tx);
        app.backend = Some(BackendHandle::new(rx, Arc::new(|| {})));
        app.is_running = true;
        app.tick();
        assert!(!app.is_running);
        assert!(app.error.is_some());
        assert_eq!(app.turns[0].text, "C1: Saved argument");
        assert_eq!(app.turns[1].status, TurnStatus::Failed);
    }

    #[test]
    fn new_stages_and_brief_do_not_take_over_the_reader() {
        let mut app = App::new(false, None, true);
        app.follow_live = false;
        app.handle_turn_start(1, "A".into(), "Opening".into());
        app.handle_turn_start(2, "B".into(), "Red team 1".into());
        assert_eq!(app.selected_turn, 0);
        app.active_tab = Tab::Claims;
        let mut brief = TurnState::new(3, "C".into(), "Brief".into());
        brief.text = "A recommendation".into();
        app.handle_brief_complete(brief);
        assert_eq!(app.active_tab, Tab::Claims);
        assert_eq!(app.brief_text, "A recommendation");
    }
    #[test]
    fn cancellation_keeps_completed_arguments() {
        let mut app = App::new(false, None, true);
        app.handle_turn_start(1, "A".into(), "Opening".into());
        app.turns[0].text = "Completed argument".into();
        app.turns[0].status = TurnStatus::Complete;
        app.start_mock_run("Offline cancellation test".into());
        app.cancel_run();
        app.tick();
        assert!(!app.is_running);
        assert_eq!(app.turns[0].text, "Completed argument");
    }
    #[test]
    fn partial_event_preserves_metadata_and_does_not_publish_claims() {
        let mut app = App::new(false, None, true);
        app.handle_turn_start(1, "m".into(), "Opening".into());
        let event = serde_json::json!({"type":"turn_failed","index":1,"model":"m","role":"Opening",
            "text":"C1: Partial claim","seconds":1,"status":"incomplete",
            "diagnostics":{"message":"Truncated","request_id":"id"}});
        app.handle_backend_event(serde_json::from_value(event).unwrap());
        app.handle_backend_event(BackendEvent::Error {
            message: "Truncated".into(),
        });
        assert!(!app.is_running);
        assert_eq!(app.turns[0].status, TurnStatus::Incomplete);
        assert_eq!(app.turns[0].diagnostics["request_id"], "id");
        assert!(app.claims.is_empty());
    }
}
