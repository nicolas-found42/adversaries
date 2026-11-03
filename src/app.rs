use std::cell::RefCell;
use std::sync::mpsc::Receiver;

use egui::Key;

use crate::claims::extract_claims;
use crate::execution::{ExecutionReview, LaunchFailure, ReviewSource, same_plan};
use crate::model::{
    BackendEvent, Claim, SessionDetail, SessionSummary, Tab, TurnState, TurnStatus,
};
use crate::transport::{BackendHandle, RunConfig, spawn, spawn_mock};
use crate::ui::composer::ComposerState;

#[cfg(not(target_arch = "wasm32"))]
use crate::session_loader::{load_session_async, scan_sessions_async};

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
    brief_availability: RefCell<crate::brief::AvailabilityCache>,
    pub claims: Vec<Claim>,
    pub selected_claim: usize,
    pub sessions: Vec<SessionSummary>,
    pub selected_session: usize,
    pub is_running: bool,
    pub status_message: String,
    /// Set when a chosen saved decision could not be read. Distinct from
    /// `error`, which belongs to a run that actually started.
    pub failed_load: Option<String>,
    pub launch_failure: Option<LaunchFailure>,
    pub(crate) load_failure_attention: bool,
    pub current_question: String,
    workspace_open: bool,
    pub session_key: String,
    pub composer_open: bool,
    pub composer_state: ComposerState,
    pub backend: Option<BackendHandle>,
    pub theme: crate::ui::theme::Theme,
    pub demo_mode: bool,
    pub should_quit: bool,
    pub follow_live: bool,
    pub workspace_generation: u64,
    pub inspector_open: bool,
    pub execution_review: Option<ExecutionReview>,
    pub stopped_by_user: bool,
    pub persisted_stages: usize,
    pub source_claim: Option<usize>,
    pub source_jump: bool,
    pub source_return: Tab,
    pub source_return_focus: Option<egui::Id>,
    pub pending_focus: Option<egui::Id>,
    pub selection_scroll: Option<Tab>,
    pub claim_search: String,
    pub history_search: String,
    pub history_error: Option<String>,
    pub total_calls: usize,
    pub error: Option<String>,
    pub catalog: Vec<crate::roles::ProviderCatalog>,
    pub catalog_message: String,
    pub markdown_cache: egui_commonmark::CommonMarkCache,
    pub last_config: Option<RunConfig>,
    pub diagnostics: serde_json::Value,
    pub stage_started: Option<web_time::Instant>,
    pending_catalog: Option<Receiver<Result<Vec<crate::roles::ProviderCatalog>, String>>>,
    pending_sessions: Option<Receiver<Result<Vec<SessionSummary>, String>>>,
    pending_detail: Option<PendingSession>,
    history_refresh_requested: bool,
    pending_preflight: Option<(
        RunConfig,
        Receiver<Result<crate::preflight::PreflightReport, String>>,
    )>,
}

struct PendingSession {
    receiver: Receiver<Option<SessionDetail>>,
    key: String,
    title: String,
    generation: u64,
    view: Tab,
    source: Option<usize>,
}

impl PendingSession {
    fn still_requested(&self, app: &App) -> bool {
        self.generation == app.workspace_generation
            && self.view == app.active_tab
            && self.source == app.source_claim
            && !app.composer_open
            && app.execution_review.is_none()
            && !app.is_running
    }
}

impl App {
    /// Initializes workspace state. The host explicitly requests initial history;
    /// constructing the model never reads the saved-decision directory.
    pub fn new(demo_mode: bool, initial_question: Option<String>, dry_run: bool) -> Self {
        let sessions = Vec::new();
        let mut app = Self {
            active_tab: Tab::Debate,
            failed_load: None,
            launch_failure: None,
            load_failure_attention: false,
            turns: Vec::new(),
            selected_turn: 0,
            brief_text: String::new(),
            brief_availability: RefCell::default(),
            claims: Vec::new(),
            selected_claim: 0,
            sessions,
            selected_session: 0,
            is_running: false,
            status_message: "Write a question to begin.".to_string(),
            current_question: initial_question.clone().unwrap_or_default(),
            workspace_open: false,
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
            workspace_generation: 0,
            inspector_open: false,
            execution_review: None,
            stopped_by_user: false,
            persisted_stages: 0,
            source_claim: None,
            source_jump: false,
            source_return: Tab::Claims,
            source_return_focus: None,
            pending_focus: None,
            selection_scroll: None,
            claim_search: String::new(),
            history_search: String::new(),
            history_error: None,
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
            history_refresh_requested: false,
            pending_preflight: None,
        };
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
        self.poll_preflight();
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
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => {
                Err("Saved-decision loading ended without a result. Refresh to retry.".into())
            }
        };
        self.pending_sessions = None;
        match result {
            Ok(sessions) => self.replace_sessions(sessions),
            Err(error) => self.history_error = Some(error),
        }
        if std::mem::take(&mut self.history_refresh_requested) {
            self.refresh_sessions();
        }
    }

    fn poll_pending_detail(&mut self) {
        let Some(pending) = &self.pending_detail else {
            return;
        };
        if !pending.still_requested(self) {
            self.cancel_session_open();
            return;
        }
        let detail = match pending.receiver.try_recv() {
            Ok(detail) => detail,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => None,
        };
        let pending = self.pending_detail.take().unwrap();
        if let Some(detail) = detail {
            self.apply_session_detail(detail);
            self.session_key = pending.key;
        } else {
            self.report_load_failure();
        }
    }

    fn handle_backend_event(&mut self, event: BackendEvent) {
        match event {
            BackendEvent::Start {
                question,
                total_calls,
                ..
            } => {
                self.total_calls = total_calls;
                self.current_question = question;
                self.status_message =
                    format!("The council started: {total_calls} arguments to come.");
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
            BackendEvent::CheckpointSaved {
                report_path,
                completed_stages,
            } => {
                let complete_count = self
                    .turns
                    .iter()
                    .filter(|turn| turn.status == TurnStatus::Complete)
                    .count();
                self.persisted_stages = completed_stages.min(complete_count);
                self.session_key = session_key(&report_path);
            }
            BackendEvent::Done { report_path, .. } => self.handle_done(report_path),
            BackendEvent::Error { message } => self.handle_run_error(message),
        }
    }

    fn handle_done(&mut self, report_path: String) {
        if !self.has_brief() {
            let message = self.error.clone().unwrap_or_else(|| {
                "The run ended without a decision brief. Completed arguments remain readable."
                    .into()
            });
            self.handle_run_error(message);
            return;
        }
        self.session_key = session_key(&report_path);
        self.is_running = false;
        self.backend = None;
        self.status_message = if self.demo_mode {
            "Practice brief ready. This sample was not saved."
        } else {
            "Decision saved. Your brief is ready."
        }
        .into();
        self.claims = extract_claims(&self.turns);
        if self.follow_live && self.active_tab == Tab::Debate && self.source_claim.is_none() {
            self.active_tab = Tab::Brief;
        }
        self.refresh_sessions();
    }

    fn handle_run_error(&mut self, message: String) {
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
        self.status_message = format!("Working on stage {index} of {}.", self.total_calls);
    }

    fn apply_completed_turn(&mut self, turn: TurnState) {
        if let Some(existing) = self.turns.iter_mut().find(|t| t.index == turn.index) {
            *existing = turn;
        } else {
            self.turns.push(turn);
        }
        let origin = self
            .source_claim
            .and_then(|i| self.claims.get(i))
            .map(crate::model::Claim::origin_key);
        self.claims = extract_claims(&self.turns);
        self.source_claim =
            origin.and_then(|key| self.claims.iter().position(|c| c.origin_key() == key));
        if let Some(index) = self.source_claim {
            self.selected_claim = index;
        }
    }

    fn handle_brief_complete(&mut self, mut turn: TurnState) {
        self.brief_text.clone_from(&turn.text);
        let available = self.has_brief();
        if !available {
            turn.status = TurnStatus::Incomplete;
            self.error = Some("The brief stage returned no readable body. Its output is kept as incomplete. Review remaining work to request a new brief; nothing is retried automatically.".into());
        }
        self.apply_completed_turn(turn);
        self.status_message = if available {
            "Your decision brief is ready.".into()
        } else {
            "No readable brief returned. Completed arguments remain available; no automatic retry."
                .into()
        };
    }

    /// Global keyboard shortcuts. Inert while the composer modal owns input.
    pub fn handle_keys(&mut self, ctx: &egui::Context) {
        ctx.input(|input| self.clear_navigation_requests(input));
        // Text entry, dialogs and popups own input. An ordinary focused button
        // alone does not disable Council's global shortcuts.
        if self.composer_open
            || self.execution_review.is_some()
            || ctx.text_edit_focused()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        ctx.input(|input| self.apply_shortcuts(input));
        if self.selection_scroll.is_some() {
            // Council owns directional selection here; do not also let egui
            // move focus to an unrelated control at the end of the pass.
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        }
    }

    fn clear_navigation_requests(&mut self, input: &egui::InputState) {
        if input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::MouseWheel { .. }
                    | egui::Event::AccessKitActionRequest(_)
                    | egui::Event::PointerButton { pressed: true, .. }
                    | egui::Event::Key { pressed: true, .. }
            )
        }) {
            self.selection_scroll = None;
            self.pending_focus = None;
            self.load_failure_attention = false;
        }
    }

    fn apply_shortcuts(&mut self, input: &egui::InputState) {
        if input.modifiers.any() {
            return;
        }
        if input.key_pressed(Key::Escape)
            && self.active_tab == Tab::Debate
            && self.source_claim.is_some()
        {
            self.return_from_source();
            return;
        }
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
    }

    fn apply_debate_shortcuts(&mut self, input: &egui::InputState) {
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

    pub fn visible_sessions(&self) -> Vec<usize> {
        let query = self.history_search.to_lowercase();
        self.sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.question.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn visible_claims(&self) -> Vec<usize> {
        self.claims
            .iter()
            .enumerate()
            .filter(|(_, c)| crate::claims::matches_query(c, &self.claim_search))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn navigate_up(&mut self) {
        self.navigate_selection(false);
    }
    pub fn navigate_down(&mut self) {
        self.navigate_selection(true);
    }

    fn navigate_selection(&mut self, forward: bool) {
        let message = match self.active_tab {
            Tab::Debate => {
                self.follow_live = false;
                self.source_claim = None;
                self.selected_turn = selection_step(
                    &(0..self.turns.len()).collect::<Vec<_>>(),
                    self.selected_turn,
                    forward,
                );
                self.turns.get(self.selected_turn).map(|turn| {
                    format!(
                        "Reading stage {} · {} · {}.",
                        turn.index, turn.role, turn.model
                    )
                })
            }
            Tab::Claims => {
                self.selected_claim =
                    selection_step(&self.visible_claims(), self.selected_claim, forward);
                self.claims.get(self.selected_claim).filter(|_| !self.visible_claims().is_empty()).map(|claim| format!("Selected {} · stage {} · {}. Debate labels are not independent verification.", claim.id, claim.turn_index, claim.author_model))
            }
            Tab::Sessions => {
                self.selected_session =
                    selection_step(&self.visible_sessions(), self.selected_session, forward);
                self.sessions
                    .get(self.selected_session)
                    .filter(|_| !self.visible_sessions().is_empty())
                    .map(|session| {
                        format!(
                            "Selected saved decision: {}. Opening it does not run models.",
                            session.question
                        )
                    })
            }
            _ => None,
        };
        if let Some(message) = message {
            self.selection_scroll = Some(self.active_tab);
            self.status_message = message;
        }
    }

    pub fn next_tab(&mut self) {
        let next_idx = (self.active_tab as usize + 1) % Tab::ALL.len();
        self.active_tab = Tab::ALL[next_idx];
    }

    /// Opening a review never cancels work or dispatches a model request.
    pub fn submit_composer(&mut self) {
        self.prepare_review(ReviewSource::Composer);
    }

    pub fn start_run(&mut self, question: String, rounds: u32, dry_run: bool) {
        self.composer_state.question = question;
        self.composer_state.rounds = rounds;
        self.composer_state.dry_run = dry_run;
        self.prepare_review(ReviewSource::Composer);
    }

    pub fn review_config(&self, source: ReviewSource) -> Option<RunConfig> {
        if source == ReviewSource::Composer {
            return Some(RunConfig {
                question: self.composer_state.question.trim().to_owned(),
                rounds: if self.demo_mode {
                    1
                } else {
                    self.composer_state.rounds
                },
                dry_run: self.demo_mode || self.composer_state.dry_run,
                roles: self.composer_state.roles.clone(),
                context_files: self.composer_state.context_files.clone(),
                max_attempts: 1,
                ..RunConfig::default()
            });
        }
        let mut config = self.last_config.clone()?;
        config.completed_turns.clear();
        config.request_id.clear();
        if source == ReviewSource::Continue {
            config.completed_turns = self
                .turns
                .iter()
                .filter(|t| t.status == TurnStatus::Complete)
                .cloned()
                .collect();
        }
        Some(config)
    }

    fn prepare_review(&mut self, source: ReviewSource) {
        let Some(config) = self.review_config(source) else {
            self.composer_state
                .question
                .clone_from(&self.current_question);
            self.composer_open = true;
            self.status_message = "Saved settings are unavailable. Configure a new run; existing arguments remain readable.".into();
            return;
        };
        if config.question.is_empty() || !(1..=5).contains(&config.rounds) {
            self.status_message = "Enter a question and choose one to five rounds.".into();
            return;
        }
        if source == ReviewSource::Continue
            && config.completed_turns.len() >= config.planned_stages()
        {
            self.status_message =
                "No unfinished stages remain. Copy the brief to preserve it, or review a new run."
                    .into();
            return;
        }
        self.composer_open = false;
        self.execution_review = Some(ExecutionReview {
            source,
            config,
            message: String::new(),
            preflight: None,
        });
        self.retry_preflight();
    }

    /// Consume approval once, and re-review when any execution input changed.
    pub fn confirm_execution(&mut self) {
        let Some(mut review) = self.execution_review.take() else {
            return;
        };
        let Some(current) = self.review_config(review.source) else {
            return;
        };
        if !same_plan(&review.config, &current) {
            review.config = current;
            review.message = "The configuration or retained work changed. Review the updated plan before starting.".into();
            review.preflight = None;
            self.execution_review = Some(review);
            self.retry_preflight();
            return;
        }
        if !review.ready() {
            review.message =
                "Finish the input check and resolve its errors before starting.".into();
            self.execution_review = Some(review);
            return;
        }
        review.config.context_snapshot = Some(
            review
                .preflight
                .as_ref()
                .unwrap()
                .documents
                .iter()
                .map(|d| d.snapshot.clone())
                .collect(),
        );
        review.config.request_id = crate::transport::new_request_id();
        self.execute_review(review);
    }

    pub fn edit_execution(&mut self) {
        let Some(review) = self.execution_review.take() else {
            return;
        };
        self.composer_state.question = review.config.question;
        self.composer_state.roles = review.config.roles;
        if self.composer_state.roles.is_empty() {
            self.composer_state.roles = crate::roles::defaults();
        }
        self.composer_state.context_files = review.config.context_files;
        self.composer_state.rounds = review.config.rounds;
        self.composer_state.dry_run = review.config.dry_run;
        self.pending_preflight = None;
        self.composer_open = true;
        self.status_message = "Editing a new run. Existing work remains readable; it will not be reused by new settings.".into();
    }

    pub fn retry_preflight(&mut self) {
        let Some(review) = self.execution_review.as_mut() else {
            return;
        };
        review.preflight = None;
        if self.demo_mode && review.config.context_files.is_empty() {
            review.preflight = Some(crate::preflight::PreflightReport {
                ready: true,
                ..Default::default()
            });
            return;
        }
        self.pending_preflight = Some((
            review.config.clone(),
            crate::preflight::check(review.config.clone()),
        ));
    }

    fn poll_preflight(&mut self) {
        let Some((config, receiver)) = &self.pending_preflight else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(report) => report,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("Input check ended without a result. Retry the check.".into()),
        };
        if let Some(review) = &mut self.execution_review
            && same_plan(config, &review.config)
        {
            match result {
                Ok(report) => {
                    review.preflight = Some(report);
                }
                Err(error) => {
                    review.message = error;
                    review.preflight = Some(crate::preflight::PreflightReport::default());
                }
            }
        }
        self.pending_preflight = None;
    }

    fn execute_review(&mut self, review: ExecutionReview) {
        self.execute_review_using(review, spawn);
    }

    fn execute_review_using(
        &mut self,
        review: ExecutionReview,
        start: impl FnOnce(RunConfig) -> Result<BackendHandle, String>,
    ) {
        self.cancel_run();
        let config = review.config;
        let handle = if self.demo_mode {
            None
        } else {
            match start(config.clone()) {
                Ok(handle) => Some(handle),
                Err(message) => {
                    self.launch_failure = Some(LaunchFailure {
                        question: config.question,
                        message,
                    });
                    self.status_message = if self.has_workspace() {
                        "New run could not start. Previous visible work remains available. No retry happens automatically."
                    } else {
                        "New run could not start. No retry happens automatically."
                    }.into();
                    return;
                }
            }
        };
        self.launch_failure = None;
        self.workspace_generation = self.workspace_generation.wrapping_add(1);
        self.follow_live = true;
        self.pending_focus = None;
        self.selection_scroll = None;
        self.source_return_focus = None;
        self.pending_detail = None;
        self.failed_load = None;
        self.load_failure_attention = false;
        self.stopped_by_user = false;
        self.source_claim = None;
        self.session_key.clear();
        self.persisted_stages = 0;
        self.error = None;
        self.turns.clone_from(&config.completed_turns);
        self.brief_text.clear();
        self.claims = extract_claims(&self.turns);
        self.diagnostics = serde_json::Value::Null;
        self.workspace_open = true;
        self.current_question.clone_from(&config.question);
        self.active_tab = Tab::Debate;
        self.selected_turn = self.turns.len().saturating_sub(1);
        self.total_calls = config.planned_stages();
        self.last_config = Some(config.clone());
        self.attach_backend(config, handle);
    }

    fn attach_backend(&mut self, config: RunConfig, handle: Option<BackendHandle>) {
        if let Some(handle) = handle {
            self.backend = Some(handle);
            self.is_running = true;
            self.status_message = "Starting the reviewed execution plan.".into();
        } else {
            self.start_mock_run(config.question);
        }
    }

    pub fn start_mock_run(&mut self, question: String) {
        self.is_running = true;
        self.status_message = "Practice example: simulated debate, no inference charges.".into();
        self.backend = Some(spawn_mock(question, false));
    }

    pub fn cancel_run(&mut self) {
        if let Some(backend) = self.backend.take() {
            backend.cancel();
            self.is_running = false;
            self.stopped_by_user = true;
            self.status_message = "Stopped by you. Completed arguments remain available here. An in-flight provider request may still be charged.".into();
            for turn in &mut self.turns {
                if turn.status == TurnStatus::Running {
                    turn.status = TurnStatus::Failed;
                }
            }
            self.refresh_sessions();
        }
    }

    pub fn rerun_current(&mut self) {
        self.prepare_review(ReviewSource::Repeat);
    }

    pub fn retry_stage(&mut self) {
        self.prepare_review(ReviewSource::Continue);
    }

    pub fn read_completed_arguments(&mut self) {
        let Some(index) = self
            .turns
            .iter()
            .position(|turn| turn.status == TurnStatus::Complete)
        else {
            return;
        };
        self.active_tab = Tab::Debate;
        self.selected_turn = index;
        self.follow_live = false;
        self.source_claim = None;
        self.source_jump = false;
        self.selection_scroll = Some(Tab::Debate);
        self.pending_focus = None;
        self.source_return_focus = None;
        self.status_message = if self.is_running {
            format!(
                "Reading completed stage {} while the run continues.",
                self.turns[index].index
            )
        } else {
            format!(
                "Reading completed stage {}. No model request was started.",
                self.turns[index].index
            )
        };
    }

    pub fn inspect_claim(&mut self, index: usize) {
        let Some(claim) = self.claims.get(index) else {
            return;
        };
        if let Some(turn) = self.turns.iter().position(|t| t.index == claim.turn_index) {
            self.selected_claim = index;
            self.source_return = self.active_tab;
            self.source_claim = Some(index);
            self.source_jump = true;
            self.selected_turn = turn;
            self.active_tab = Tab::Debate;
            self.follow_live = false;
            self.status_message = format!(
                "Inspecting {} in its originating argument. Debate outcome is not external verification.",
                claim.id
            );
        }
    }

    pub fn return_from_source(&mut self) {
        if let Some(index) = self.source_claim.take() {
            self.selected_claim = index;
            self.active_tab = self.source_return;
            self.pending_focus = self.source_return_focus.take();
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
            || self.pending_preflight.is_some()
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
        if self.session_opening() {
            return;
        }
        if self.is_running {
            self.status_message =
                "A run is active. Stop it or wait before opening another decision.".into();
            return;
        }
        self.cancel_run();
        self.failed_load = None;
        self.execution_review = None;
        #[cfg(not(target_arch = "wasm32"))]
        let receiver = load_session_async(path.to_owned());
        #[cfg(target_arch = "wasm32")]
        let receiver = fetch_session_async(path.to_owned());
        self.begin_session_open(path, receiver);
    }

    fn begin_session_open(&mut self, path: &str, receiver: Receiver<Option<SessionDetail>>) {
        let title = self
            .sessions
            .iter()
            .find(|session| session.path_str == path)
            .map_or_else(
                || "Saved decision".into(),
                |session| session.question.clone(),
            );
        self.pending_detail = Some(PendingSession {
            receiver,
            key: session_key(path),
            title,
            generation: self.workspace_generation,
            view: self.active_tab,
            source: self.source_claim,
        });
        self.status_message = "Opening saved decision…".into();
    }

    pub fn session_opening(&self) -> bool {
        self.pending_detail.is_some()
    }

    pub fn opening_question(&self) -> Option<&str> {
        self.pending_detail
            .as_ref()
            .map(|pending| pending.title.as_str())
    }

    pub fn cancel_session_open(&mut self) {
        if self.pending_detail.take().is_none() {
            return;
        }
        self.status_message = "Opening cancelled. Previous visible work was not replaced.".into();
        if self.active_tab == Tab::Sessions {
            self.selection_scroll = Some(Tab::Sessions);
        }
    }

    /// A decision that could not be read is not open: say so and drop the
    /// highlight rather than leaving the sidebar pointing at nothing.
    fn report_load_failure(&mut self) {
        self.load_failure_attention = true;
        self.selection_scroll = None;
        self.pending_focus = None;
        self.session_key.clear();
        self.status_message = "That saved decision could not be read.".into();
        self.failed_load = Some(
            "That saved decision could not be read. Check storage or the Council server and try again. Previous visible work was not replaced.".into(),
        );
    }

    /// True when a saved decision has arguments but no brief: readable, not done.
    /// True when a saved decision was chosen but could not be read. Nothing was
    /// opened, so no row may stay highlighted and no brief may be claimed.
    pub fn load_failed(&self) -> bool {
        self.failed_load.is_some()
    }

    pub fn has_workspace(&self) -> bool {
        self.workspace_open
            || !self.current_question.trim().is_empty()
            || !self.turns.is_empty()
            || self.has_brief()
            || self.is_running
    }

    pub fn has_brief(&self) -> bool {
        self.brief_availability.borrow_mut().check(&self.brief_text)
    }

    pub fn needs_brief(&self) -> bool {
        self.has_workspace() && !self.has_brief() && !self.is_running
    }

    fn apply_session_detail(&mut self, detail: SessionDetail) {
        self.launch_failure = None;
        self.failed_load = None;
        self.load_failure_attention = false;
        self.workspace_open = true;
        self.workspace_generation = self.workspace_generation.wrapping_add(1);
        self.pending_focus = None;
        self.selection_scroll = None;
        self.source_return_focus = None;
        self.last_config = detail.run_config;
        self.execution_review = None;
        self.source_claim = None;
        self.stopped_by_user = false;
        self.diagnostics = detail.diagnostics;
        // Opening is not a new run failure. Drop the previous workspace's error;
        // restore only this record's interrupted-stage context below.
        self.error = None;
        self.total_calls = self
            .last_config
            .as_ref()
            .map_or(detail.turns.len() + 1, RunConfig::planned_stages);
        if let Some(config) = &self.last_config {
            self.composer_state.question.clone_from(&config.question);
            self.composer_state.roles.clone_from(&config.roles);
            self.composer_state.rounds = config.rounds;
            self.composer_state.dry_run = config.dry_run;
            self.composer_state
                .context_files
                .clone_from(&config.context_files);
        }
        self.current_question = detail.question;
        self.brief_text = detail.brief;
        self.turns = detail.turns;
        let interruption = crate::model::StageInterruption::from_turns(&self.turns);
        self.error = interruption
            .as_ref()
            .map(crate::model::StageInterruption::description);
        self.persisted_stages = self
            .turns
            .iter()
            .filter(|t| t.status == TurnStatus::Complete)
            .count();
        self.claims = extract_claims(&self.turns);
        self.selected_turn = 0;
        self.status_message = if interruption.is_some() {
            "Saved interruption opened. Opening it sent no new model requests.".into()
        } else if self.has_brief() {
            "Saved decision opened. Your brief is ready.".into()
        } else {
            "Saved decision opened. It has no brief yet.".into()
        };
        self.active_tab = if self.has_brief() {
            Tab::Brief
        } else {
            Tab::Debate
        };
    }

    pub fn history_loading(&self) -> bool {
        self.pending_sessions.is_some()
    }

    fn replace_sessions(&mut self, sessions: Vec<SessionSummary>) {
        let selected_path = self
            .sessions
            .get(self.selected_session)
            .map(|s| &s.path_str);
        let retained =
            selected_path.and_then(|path| sessions.iter().position(|s| &s.path_str == path));
        if selected_path.is_some() && retained.is_none() {
            self.status_message = "The selected saved decision is no longer listed. Choose another record; your open work has not changed.".into();
        }
        self.selected_session =
            retained.unwrap_or_else(|| self.selected_session.min(sessions.len().saturating_sub(1)));
        self.sessions = sessions;
        self.history_error = None;
    }

    pub fn refresh_sessions(&mut self) {
        if self.pending_sessions.is_some() {
            self.history_refresh_requested = true;
            return;
        }
        self.history_error = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.pending_sessions = Some(scan_sessions_async(reports_directory()));
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

fn selection_step(visible: &[usize], selected: usize, forward: bool) -> usize {
    let Some(position) = visible.iter().position(|i| *i == selected) else {
        return visible.first().copied().unwrap_or(selected);
    };
    let next = if forward {
        (position + 1).min(visible.len() - 1)
    } else {
        position.saturating_sub(1)
    };
    visible[next]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc::channel};

    #[test]
    fn completion_without_a_brief_never_claims_a_recommendation_is_ready() {
        let mut app = App::new(false, None, true);
        app.handle_done("fixture.md".into());
        assert!(app.brief_text.is_empty());
        assert!(
            app.error
                .as_ref()
                .unwrap()
                .contains("without a decision brief")
        );
        assert!(!app.status_message.contains("brief is ready"));
    }

    #[test]
    fn unavailable_brief_retains_completed_arguments_and_can_be_reviewed_for_continuation() {
        for text in ["", " \n\t", "# Decision Brief\n\n---"] {
            let mut app = App::new(false, None, true);
            app.current_question = "Synthetic A or B?".into();
            let assignments = crate::roles::defaults();
            app.last_config = Some(RunConfig {
                question: app.current_question.clone(),
                rounds: 1,
                dry_run: true,
                roles: assignments.clone(),
                ..RunConfig::default()
            });
            app.is_running = true;
            for (offset, (key, role)) in [
                ("opening", "Opening"),
                ("critique_1", "Red team 1"),
                ("critique_2", "Red team 2"),
                ("critique_3", "Red team 3"),
                ("rebuttal", "Rebuttal"),
            ]
            .into_iter()
            .enumerate()
            {
                let index = offset + 1;
                let selection = &assignments[key];
                let mut turn = TurnState::new(
                    index,
                    format!("{} / {}", selection.provider, selection.model),
                    role.into(),
                );
                turn.status = TurnStatus::Complete;
                turn.text = format!("C{index}: Completed fixture argument.");
                app.turns.push(turn);
            }
            let mut brief = TurnState::new(6, "Fixture writer / exact-id".into(), "Brief".into());
            brief.status = TurnStatus::Complete;
            brief.text = text.into();
            brief.seconds = 1.25;
            brief.diagnostics =
                serde_json::json!({"request_id":"synthetic-request", "finish_reason":"stop"});
            app.handle_brief_complete(brief);
            assert_eq!(app.turns[5].status, TurnStatus::Incomplete);
            assert_eq!(app.turns[5].text, text);
            assert_eq!(app.turns[5].diagnostics["request_id"], "synthetic-request");
            app.handle_backend_event(BackendEvent::CheckpointSaved {
                report_path: "fixture.md".into(),
                completed_stages: 6,
            });
            assert_eq!(app.persisted_stages, 5);
            app.handle_done("fixture.md".into());
            assert!(app.error.as_deref().unwrap().contains("no readable body"));
            assert!(!app.is_running);
            app.retry_stage();
            let review = app
                .execution_review
                .as_ref()
                .expect("remaining brief must be reviewable");
            assert_eq!(review.retained(), 5);
            assert_eq!(review.remaining(), 1);
            let started = std::time::Instant::now();
            while !app.execution_review.as_ref().unwrap().ready()
                && started.elapsed() < std::time::Duration::from_secs(10)
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
                app.tick();
            }
            assert!(
                app.execution_review.as_ref().unwrap().ready(),
                "the actual preflight must accept the retained prefix"
            );
            assert!(app.backend.is_none(), "no automatic retry");
        }
    }

    #[test]
    fn readable_structured_and_unstructured_briefs_keep_normal_completion() {
        for text in [
            "Choose A, with conditions.",
            "# Decision Brief\n\n## Recommendation\n\nChoose A.",
        ] {
            let mut app = App::new(false, None, true);
            app.is_running = true;
            let mut brief = TurnState::new(1, "Fixture writer".into(), "Brief".into());
            brief.status = TurnStatus::Complete;
            brief.text = text.into();
            app.handle_brief_complete(brief);
            app.handle_backend_event(BackendEvent::CheckpointSaved {
                report_path: "fixture.md".into(),
                completed_stages: 1,
            });
            app.handle_done("fixture.md".into());
            assert!(app.error.is_none());
            assert_eq!(app.turns[0].status, TurnStatus::Complete);
            assert_eq!(app.brief_text, text);
            assert_eq!(app.persisted_stages, 1);
            assert_eq!(app.active_tab, Tab::Brief);
            assert!(app.backend.is_none());
        }
    }

    #[test]
    fn completion_opens_brief_only_for_a_viewer_following_progress() {
        for (tab, following, expected) in [
            (Tab::Debate, true, Tab::Brief),
            (Tab::Debate, false, Tab::Debate),
            (Tab::Claims, true, Tab::Claims),
        ] {
            let mut app = App::new(false, None, true);
            app.brief_text = "A generated recommendation".into();
            app.active_tab = tab;
            app.follow_live = following;
            app.handle_done("fixture.md".into());
            assert_eq!(app.active_tab, expected);
        }
    }

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
    fn opening_an_unfinished_decision_is_not_a_failure() {
        let mut app = App::new(false, None, true);
        app.error = Some("A previous failure".into());
        let mut turn = TurnState::new(1, "m".into(), "Opening".into());
        turn.text = "Saved argument".into();
        turn.status = TurnStatus::Complete;
        app.apply_session_detail(SessionDetail {
            question: "Saved question".into(),
            brief: String::new(),
            turns: vec![turn],
            run_config: None,
            diagnostics: serde_json::Value::Null,
        });
        assert_eq!(app.error, None, "opening a file is not a failure");
        assert!(app.status_message.contains("no brief"));
        assert!(
            app.needs_brief(),
            "the reader must be told how to finish it"
        );
    }

    #[test]
    fn an_unreadable_decision_reports_instead_of_staying_silent() {
        let mut app = App::new(false, None, true);
        app.session_key = "2026-10-23_1910_q".into();
        app.load_session_by_path("/nonexistent/decision.json");
        super::storage_tests::finish_open(&mut app);
        assert!(app.load_failed(), "the reader must be told the load failed");
        assert!(
            app.session_key.is_empty(),
            "no row may stay highlighted for a decision that never opened"
        );
        assert!(
            app.status_message.contains("could not be read"),
            "the footer must not keep claiming it is opening"
        );
        assert!(!app.needs_brief(), "nothing was opened");
        assert_eq!(app.error, None, "a missing file is not a failed run");
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

    fn history_record(path: &str) -> SessionSummary {
        serde_json::from_value(serde_json::json!({
            "file_name":path,"path_str":path,"created_at":"","question":path,
            "rounds":1,"turn_count":0,"has_brief":false
        }))
        .unwrap()
    }

    #[test]
    fn history_refresh_preserves_selected_identity_when_rows_move() {
        let mut app = App::new(false, None, true);
        app.sessions = vec![history_record("A"), history_record("B")];
        app.selected_session = 1;
        app.current_question = "Open work".into();
        let (tx, rx) = std::sync::mpsc::channel();
        app.pending_sessions = Some(rx);
        tx.send(Ok(vec![
            history_record("new"),
            history_record("A"),
            history_record("B"),
        ]))
        .unwrap();
        app.poll_pending_sessions();
        assert_eq!(app.selected_session, 2);
        assert_eq!(app.sessions[app.selected_session].path_str, "B");
        assert_eq!(app.current_question, "Open work");
        assert!(app.backend.is_none());
        assert!(app.pending_sessions.is_none());
    }

    #[test]
    fn history_refresh_explains_removed_selection_without_replacing_open_work() {
        let mut app = App::new(false, None, true);
        app.sessions = vec![history_record("A"), history_record("B")];
        app.selected_session = 1;
        app.current_question = "Open work".into();
        app.replace_sessions(vec![history_record("A")]);
        assert_eq!(app.selected_session, 0);
        assert!(app.status_message.contains("no longer listed"));
        assert_eq!(app.current_question, "Open work");
        app.replace_sessions(vec![]);
        assert!(app.sessions.is_empty());
        assert_eq!(app.selected_session, 0);
        assert!(app.backend.is_none());
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod storage_tests;

#[cfg(test)]
mod launch_tests;
