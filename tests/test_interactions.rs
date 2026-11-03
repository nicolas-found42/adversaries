//! Pointer/keyboard journeys through the actual shared egui views.
mod common;
use council_ui::{
    app::App,
    model::{Tab, TurnState, TurnStatus},
    transport::RunConfig,
};
use egui::{Event, Modifiers, PointerButton, Pos2};
use std::fmt::Write;

struct Surface {
    ctx: egui::Context,
    app: App,
    labels: Vec<(String, egui::Rect)>,
    viewport: egui::Vec2,
    copied: Vec<String>,
}

impl Surface {
    fn new() -> Self {
        let mut app = App::new(false, None, true);
        app.catalog_message = "Offline interaction fixture; catalog not fetched.".into();
        Self {
            ctx: egui::Context::default(),
            app,
            labels: Vec::new(),
            viewport: egui::vec2(620.0, 900.0),
            copied: Vec::new(),
        }
    }

    fn frame(&mut self, events: Vec<Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                self.viewport / self.ctx.zoom_factor(),
            )),
            events,
            ..Default::default()
        };
        let mut output = self.ctx.run_ui(input, |ui| {
            self.app.handle_keys(ui.ctx());
            council_ui::ui::render(ui, &mut self.app);
        });
        self.labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => {
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    shape
                        .clip_rect
                        .contains(rect.center())
                        .then(|| (text.galley.text().to_owned(), rect))
                }
                _ => None,
            })
            .collect();
        for command in &output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                self.copied.push(text.clone());
            }
        }
        output.textures_delta.clear();
    }

    fn settle(&mut self) {
        common::finish_storage(&mut self.app);
        for _ in 0..4 {
            self.frame(Vec::new());
        }
    }

    fn click(&mut self, label: &str) {
        let position = self.reveal(label).center();
        for pressed in [true, false] {
            self.frame(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        }
        self.settle();
    }

    fn reveal(&mut self, label: &str) -> egui::Rect {
        self.settle();
        for _ in 0..30 {
            if let Some((_, rect)) = self
                .labels
                .iter()
                .find(|(text, rect)| text == label && self.ctx.content_rect().contains_rect(*rect))
            {
                return *rect;
            }
            self.scroll(-180.0);
        }
        panic!("{label:?} is not reachable inside the viewport");
    }

    fn press(&mut self, key: egui::Key) {
        for pressed in [true, false] {
            self.frame(vec![Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
        }
        self.settle();
    }

    fn scroll(&mut self, delta: f32) {
        let position = self.ctx.content_rect().center();
        self.frame(vec![
            Event::PointerMoved(position),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                delta: egui::vec2(0.0, delta),
                modifiers: Modifiers::NONE,
            },
        ]);
        for _ in 0..15 {
            self.frame(Vec::new());
        }
    }

    fn seed_statement(&mut self) {
        self.app.current_question = "Should we choose A or B?".into();
        let mut turn = TurnState::new(1, "Mistral / mistral-small-latest".into(), "Opening".into());
        turn.text = "C1: This is a claim, not an externally verified fact.".into();
        turn.status = TurnStatus::Complete;
        self.app.turns = vec![turn];
        self.app.claims = council_ui::claims::extract_claims(&self.app.turns);
        self.app.brief_text = "# Recommendation\n\nFixture recommendation.".into();
        self.app.follow_live = false;
        self.app.active_tab = Tab::Claims;
    }
}

#[test]
fn composer_review_and_cancel_do_not_dispatch_inference() {
    let mut surface = Surface::new();
    surface.click("New decision");
    surface.app.composer_state.question = "Should we use A or B?".into();
    surface.click("Review run");
    assert!(surface.app.execution_review.is_some());
    assert!(surface.app.backend.is_none());
    surface.click("Cancel");
    assert!(surface.app.execution_review.is_none());
    assert!(surface.app.backend.is_none());
}

#[test]
fn source_back_restores_selection_and_keyboard_focus() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.click("Inspect statement, challenges and response");
    let opener = surface.app.source_return_focus.unwrap();
    assert_eq!(surface.app.source_claim, Some(0));
    surface.click("< Back to Claims & sources");
    assert_eq!(surface.app.active_tab, Tab::Claims);
    assert_eq!(surface.app.selected_claim, 0);
    assert_eq!(surface.ctx.memory(egui::Memory::focused), Some(opener));
}

#[test]
fn escape_returns_from_source_without_triggering_a_run() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.click("Inspect statement, challenges and response");
    surface.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    surface.settle();
    assert_eq!(surface.app.active_tab, Tab::Claims);
    assert!(surface.app.source_claim.is_none());
    assert!(surface.app.backend.is_none());
}

#[test]
fn failure_continuation_review_preserves_readable_work() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.app.error = Some("Fixture provider timeout".into());
    surface.app.brief_text.clear();
    surface.app.last_config = Some(RunConfig {
        question: surface.app.current_question.clone(),
        rounds: 1,
        dry_run: true,
        roles: council_ui::roles::defaults(),
        ..Default::default()
    });
    surface.click("Review remaining work");
    let review = surface.app.execution_review.as_ref().unwrap();
    assert_eq!(review.retained(), 1);
    assert_eq!(review.remaining(), 5);
    assert_eq!(surface.app.turns.len(), 1);
    assert!(surface.app.backend.is_none());
    surface.click("Cancel");
    assert_eq!(
        surface.app.turns[0].text,
        "C1: This is a claim, not an externally verified fact."
    );
}

#[test]
fn enlarged_text_keeps_source_navigation_reachable() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.ctx.set_zoom_factor(1.5);
    surface.click("Inspect statement, challenges and response");
    assert!(surface.app.source_claim.is_some());
    surface.click("< Back to Claims & sources");
    assert_eq!(surface.app.active_tab, Tab::Claims);
    assert!(surface.app.source_claim.is_none());
}

#[test]
fn source_return_preserves_the_claim_reader_scroll_position() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.app.claims[0]
        .text
        .push_str(&" Supporting detail.".repeat(30));
    surface.settle();
    surface.scroll(-100.0);
    let before = surface
        .labels
        .iter()
        .find(|(text, _)| text == "C1")
        .unwrap()
        .1;
    surface.click("Inspect statement, challenges and response");
    surface.click("< Back to Claims & sources");
    let after = surface
        .labels
        .iter()
        .find(|(text, _)| text == "C1")
        .unwrap()
        .1;
    assert!(
        (before.min.y - after.min.y).abs() < 1.0,
        "reader moved: {before:?} to {after:?}"
    );
}

#[test]
fn source_inspection_leads_with_the_selected_claim_instead_of_the_sequence() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.click("Inspect statement, challenges and response");
    assert!(
        surface
            .labels
            .iter()
            .any(|(label, _)| label == "< Back to Claims & sources")
    );
    assert!(
        !surface
            .labels
            .iter()
            .any(|(label, _)| label.contains("Stage sequence"))
    );
}

#[test]
fn failed_save_after_brief_offers_copy_instead_of_repeating_inference() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.app.active_tab = Tab::Brief;
    surface.app.error = Some("Fixture final save failed".into());
    surface.settle();
    assert!(
        surface
            .labels
            .iter()
            .any(|(label, _)| label == "Run interrupted — brief available here")
    );
    surface.click("Copy brief for safekeeping");
    assert!(surface.app.backend.is_none());
    assert!(
        !surface
            .labels
            .iter()
            .any(|(label, _)| label == "Review remaining work")
    );
}

#[test]
fn keyboard_claim_selection_brings_a_distant_claim_into_the_viewport() {
    let mut surface = Surface::new();
    surface.seed_statement();
    let mut text = String::new();
    for number in 1..=20 {
        writeln!(
            text,
            "C{number}: A distinct statement requiring source inspection."
        )
        .unwrap();
    }
    surface.app.turns[0].text = text;
    surface.app.claims = council_ui::claims::extract_claims(&surface.app.turns);
    surface.settle();
    for _ in 0..19 {
        for pressed in [true, false] {
            surface.frame(vec![Event::Key {
                key: egui::Key::J,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
        }
        surface.settle();
    }
    assert_eq!(surface.app.selected_claim, 19);
    assert!(
        surface.labels.iter().any(|(label, _)| label == "C20"),
        "the selected claim must actually be visible"
    );
    assert!(surface.app.status_message.contains("Selected C20"));
}

#[test]
fn keyboard_history_selection_keeps_the_selected_row_visible() {
    let mut surface = Surface::new();
    surface.app.active_tab = Tab::Sessions;
    surface.app.sessions = (1..=20).map(|number| council_ui::model::SessionSummary {
        question: format!("Archived decision {number}: a deliberately long decision question for the viewport test"),
        file_name: format!("fixture-{number}.json"), path_str: format!("reports/fixture-{number}.json"),
        created_at: "2026-11-02T12:00:00Z".into(), rounds: 1, turn_count: 6, has_brief: true, interruption: None, dry_run: Some(true),
    }).collect();
    surface.settle();
    for _ in 0..19 {
        for pressed in [true, false] {
            surface.frame(vec![Event::Key {
                key: egui::Key::ArrowDown,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
        }
        surface.settle();
    }
    assert_eq!(surface.app.selected_session, 19);
    assert!(
        surface
            .labels
            .iter()
            .any(|(label, _)| label.starts_with("Archived decision 20:") && label.ends_with('…')),
        "the shortened row title must be visible, not just the selected preview"
    );
    assert!(
        surface
            .app
            .status_message
            .contains("Opening it does not run models")
    );
}

#[test]
fn keyboard_argument_navigation_starts_at_the_new_argument_heading() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.app.active_tab = Tab::Debate;
    for turn in &mut surface.app.turns {
        turn.text = "A long argument paragraph.\n\n".repeat(100);
    }
    let mut next = surface.app.turns[0].clone();
    next.index = 2;
    next.role = "Red team".into();
    surface.app.turns.push(next);
    surface.settle();
    surface.scroll(-500.0);
    assert!(
        !surface
            .labels
            .iter()
            .any(|(label, _)| label == "Reading: stage 1 · Opening")
    );
    surface.frame(vec![Event::Key {
        key: egui::Key::J,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    for _ in 0..20 {
        surface.frame(Vec::new());
    }
    assert_eq!(surface.app.selected_turn, 1);
    assert!(
        surface
            .labels
            .iter()
            .any(|(label, _)| label == "Reading: stage 2 · Red team"),
        "selected argument heading must be visible after navigation"
    );
}

#[test]
fn narrow_composer_keeps_review_and_cancel_reachable_without_clipping() {
    for scale in [1.0, 1.5] {
        let mut surface = Surface::new();
        surface.viewport = egui::vec2(320.0, 900.0);
        surface.ctx.set_zoom_factor(scale);
        surface.app.composer_open = true;
        surface.app.composer_state.question = "Should we choose A or B?".into();
        surface.reveal("Review run");
        surface.reveal("Cancel");
        surface.click("Review run");
        assert!(surface.app.execution_review.is_some());
        assert!(surface.app.backend.is_none());
        surface.reveal("Back to edit");
        surface.click("Cancel");
        assert!(surface.app.execution_review.is_none());
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn expanded_narrow_composer_can_remove_documents_and_reach_the_review() {
    for scale in [1.0, 1.5] {
        let mut surface = Surface::new();
        surface.viewport = egui::vec2(320.0, 900.0);
        surface.ctx.set_zoom_factor(scale);
        surface.app.composer_open = true;
        surface.app.composer_state.question = "Should we choose A or B?".into();
        surface.app.composer_state.context_files =
            vec![format!("/notes/{}.pdf", "long-name".repeat(12))];
        surface.click("Remove");
        assert!(surface.app.composer_state.context_files.is_empty());
        surface.click("Change the models");
        surface.reveal("Opening analyst");
        surface.click("Mistral");
        surface.click("OpenRouter");
        assert_eq!(
            surface.app.composer_state.roles["opening"].provider,
            "OpenRouter"
        );
        surface.reveal("Decision brief");
        surface.click("Review run");
        assert!(surface.app.execution_review.is_some());
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn narrow_review_exposes_both_execution_modes_without_starting_them() {
    for scale in [1.0, 1.5] {
        for dry_run in [true, false] {
            let mut surface = Surface::new();
            surface.viewport = egui::vec2(320.0, 900.0);
            surface.ctx.set_zoom_factor(scale);
            surface.app.execution_review = Some(council_ui::execution::ExecutionReview {
                source: council_ui::execution::ReviewSource::Composer,
                config: RunConfig {
                    question: "A long question with several options and important constraints. "
                        .repeat(8),
                    dry_run,
                    roles: council_ui::roles::defaults(),
                    ..Default::default()
                },
                preflight: Some(council_ui::preflight::PreflightReport {
                    ready: true,
                    ..Default::default()
                }),
                message: String::new(),
            });
            let label = surface
                .app
                .execution_review
                .as_ref()
                .unwrap()
                .action_label();
            surface.reveal(label);
            surface.click("Cancel");
            assert!(surface.app.execution_review.is_none());
            assert!(surface.app.backend.is_none());
        }
    }
}

#[test]
fn compact_navigation_preserves_creation_help_and_history_without_dispatch() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.click("Navigate");
    surface.click("New decision");
    assert!(surface.app.composer_open);
    surface.click("Cancel");
    surface.click("Navigate");
    surface.click("Help");
    assert_eq!(surface.app.active_tab, Tab::Help);
    surface.click("Navigate");
    surface.click("Decisions");
    assert_eq!(surface.app.active_tab, Tab::Sessions);
    assert!(surface.app.backend.is_none());
}

#[test]
fn unfinished_brief_has_one_recovery_message_and_direct_reading_at_large_text() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.app.brief_text.clear();
    surface.app.active_tab = Tab::Brief;
    surface.settle();
    assert!(surface.labels.iter().any(|(text, _)| text == "Unfinished"));
    assert!(!surface.labels.iter().any(|(text, _)| text == "Ready"));
    surface.reveal("Unfinished — no decision brief");
    assert!(
        !surface
            .labels
            .iter()
            .any(|(text, _)| text == "No decision brief is available")
    );
    surface.click("Copy completed arguments");
    let copied = surface.copied.last().unwrap();
    assert!(copied.contains("Incomplete debate — no decision brief"));
    assert!(copied.contains("C1: This is a claim, not an externally verified fact."));
    surface.click("Read completed arguments");
    assert_eq!(surface.app.active_tab, Tab::Debate);
    assert!(!surface.app.follow_live);
    assert!(surface.app.backend.is_none());
}

#[test]
fn narrow_interruption_reviews_retained_work_then_copies_without_execution() {
    for failed in [false, true] {
        let mut surface = Surface::new();
        surface.seed_statement();
        surface.viewport = egui::vec2(320.0, 900.0);
        surface.ctx.set_zoom_factor(1.5);
        surface.app.active_tab = Tab::Brief;
        surface.app.brief_text.clear();
        surface.app.error = failed.then(|| "A staged provider timeout.".into());
        surface.app.stopped_by_user = !failed;
        surface.app.last_config = Some(RunConfig {
            question: surface.app.current_question.clone(),
            roles: council_ui::roles::defaults(),
            dry_run: true,
            rounds: 1,
            ..Default::default()
        });
        surface.click("Review remaining work");
        let review = surface.app.execution_review.as_ref().unwrap();
        assert_eq!(review.retained(), 1);
        assert_eq!(review.remaining(), 5);
        surface.click("Cancel");
        surface.click("Copy completed arguments");
        assert!(
            surface
                .copied
                .last()
                .unwrap()
                .contains("Incomplete debate — no decision brief")
        );
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn copying_an_incomplete_argument_keeps_its_warning_in_the_clipboard_command() {
    let mut surface = Surface::new();
    surface.seed_statement();
    let mut partial = TurnState::new(2, "Provider / exact-id".into(), "Critique".into());
    partial.status = TurnStatus::Incomplete;
    partial.text = "A partial response that must not look complete.".into();
    surface.app.turns.push(partial);
    surface.app.selected_turn = 1;
    surface.app.active_tab = Tab::Debate;
    surface.click("Stage provenance and technical details");
    surface.click("Copy this argument");
    let copy = surface.copied.last().unwrap();
    assert!(copy.contains("Incomplete stage output"));
    assert!(copy.ends_with("A partial response that must not look complete."));
    assert!(surface.app.backend.is_none());
}

#[test]
fn compact_history_selection_does_not_move_the_remaining_rows() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(500.0, 1200.0);
    surface.app.active_tab = Tab::Sessions;
    surface.app.sessions = ["First saved decision", "Interrupted saved decision"]
        .iter()
        .enumerate()
        .map(|(index, question)| council_ui::model::SessionSummary {
            file_name: format!("synthetic-{index}.json"),
            path_str: format!("synthetic-{index}.json"),
            question: (*question).into(),
            created_at: "2026-11-02T12:00:00Z".into(),
            rounds: 1,
            turn_count: 1,
            has_brief: false,
            dry_run: Some(true),
            interruption: (index == 1).then(|| council_ui::model::StageInterruption {
                stage_index: 2,
                role: "Critique".into(),
                model: "Fixture".into(),
                reason: Some("A long recorded failure detail. ".repeat(35)),
            }),
        })
        .collect();
    surface.settle();
    let before = surface
        .labels
        .iter()
        .rev()
        .find(|(text, _)| text.contains("1 stage") && text.contains("Unfinished · no brief"))
        .unwrap()
        .1;
    surface.click("Interrupted saved decision");
    let after = surface
        .labels
        .iter()
        .rev()
        .find(|(text, _)| text.contains("1 stage") && text.contains("Unfinished · no brief"))
        .expect("selecting a row must not move the other rows out of view")
        .1;
    assert!(
        (before.min.y - after.min.y).abs() < 1.0,
        "row moved: {before:?} to {after:?}"
    );
    assert_eq!(surface.app.selected_session, 1);
    assert!(surface.app.backend.is_none());
}

#[test]
fn first_visit_and_unreadable_save_actions_fit_a_narrow_enlarged_workspace() {
    for failed_open in [false, true] {
        let mut surface = Surface::new();
        surface.viewport = egui::vec2(320.0, 900.0);
        surface.ctx.set_zoom_factor(1.5);
        if failed_open {
            surface
                .app
                .load_session_by_path("/missing/synthetic-session.json");
        }
        let action = if failed_open {
            "See all decisions"
        } else {
            "Start a decision"
        };
        surface.settle();
        assert!(
            surface.labels.iter().any(|(text, _)| text == action),
            "the next action should be visible without scrolling"
        );
        surface.reveal(action);
        for (text, bounds) in &surface.labels {
            assert!(
                bounds.max.x <= surface.ctx.content_rect().right() + 0.5,
                "unreadable text outside the narrow workspace: {bounds:?} {text}"
            );
        }
        surface.click(action);
        if failed_open {
            assert_eq!(surface.app.active_tab, Tab::Sessions);
        } else {
            assert!(surface.app.composer_open);
        }
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn a_failed_history_open_preserves_source_context_and_focuses_recovery() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.app.inspect_claim(0);
    surface.settle();
    let previous = surface.app.current_question.clone();
    surface.app.stopped_by_user = true;
    surface.app.active_tab = Tab::Sessions;
    surface.app.sessions = vec![council_ui::model::SessionSummary {
        file_name: "missing.json".into(),
        path_str: "/missing/synthetic-session.json".into(),
        question: "Missing fixture?".into(),
        created_at: "Unknown".into(),
        rounds: 1,
        turn_count: 1,
        has_brief: false,
        interruption: None,
        dry_run: Some(true),
    }];
    surface.click("Open selected decision");
    assert!(surface.app.load_failed());
    assert_eq!(surface.app.source_claim, Some(0));
    assert!(surface.app.stopped_by_user);
    assert_eq!(surface.app.current_question, previous);
    let recovery = surface.reveal("Refresh saved decisions");
    let focus = surface
        .ctx
        .memory(egui::Memory::focused)
        .expect("recovery should receive keyboard focus");
    assert!(
        surface
            .ctx
            .read_response(focus)
            .unwrap()
            .rect
            .contains(recovery.center())
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("another.json");
    std::fs::write(
        &path,
        r#"{"question":"Another decision?","brief":"Choose B.","turns":[]}"#,
    )
    .unwrap();
    surface.app.sessions[0].path_str = path.to_string_lossy().into();
    surface.settle();
    surface.press(egui::Key::Enter);
    assert_eq!(
        surface.app.active_tab,
        Tab::Sessions,
        "Enter on Refresh must not open the selected record"
    );
    assert_eq!(surface.app.current_question, previous);
    assert_eq!(surface.app.source_claim, Some(0));
    assert!(surface.app.backend.is_none());
}

#[test]
fn keyboard_record_activation_opens_saved_work_without_starting_models() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("record.json"), r#"{"question":"Saved keyboard fixture?","brief":"A synthetic recommendation.","turns":[]}"#).unwrap();
    let mut surface = Surface::new();
    surface.app.active_tab = Tab::Sessions;
    surface.app.sessions = council_ui::session_loader::scan_sessions(dir.path());
    surface.settle();
    surface.press(egui::Key::ArrowDown);
    let focus = surface
        .ctx
        .memory(egui::Memory::focused)
        .expect("selected row must have focus");
    let row = surface
        .labels
        .iter()
        .find(|(text, _)| text == "Saved keyboard fixture?")
        .unwrap()
        .1;
    assert!(
        surface
            .ctx
            .read_response(focus)
            .unwrap()
            .rect
            .contains(row.center()),
        "focus must remain on the record row: row={row:?}, focus={:?}",
        surface.ctx.read_response(focus).unwrap().rect
    );
    surface.press(egui::Key::Enter);
    assert_eq!(surface.app.active_tab, Tab::Brief);
    assert_eq!(surface.app.current_question, "Saved keyboard fixture?");
    assert_eq!(surface.app.brief_text, "A synthetic recommendation.");
    assert!(!surface.app.is_running);
    assert!(surface.app.backend.is_none());
}

#[test]
fn an_open_navigation_menu_owns_arrows_and_escape_instead_of_the_history_reader() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.app.inspect_claim(0);
    surface.app.active_tab = Tab::Sessions;
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.app.sessions = (0..2)
        .map(|index| council_ui::model::SessionSummary {
            file_name: format!("fixture-{index}.json"),
            path_str: format!("fixture-{index}.json"),
            question: format!("Saved fixture {index}?"),
            created_at: "Unknown".into(),
            rounds: 1,
            turn_count: 1,
            has_brief: false,
            interruption: None,
            dry_run: Some(true),
        })
        .collect();
    surface.click("Navigate");
    assert!(egui::Popup::is_any_open(&surface.ctx));
    surface.press(egui::Key::ArrowDown);
    assert_eq!(surface.app.selected_session, 0);
    surface.press(egui::Key::Escape);
    assert!(!egui::Popup::is_any_open(&surface.ctx));
    assert_eq!(surface.app.active_tab, Tab::Sessions);
    assert_eq!(surface.app.source_claim, Some(0));
    surface.press(egui::Key::Escape);
    assert_eq!(
        surface.app.active_tab,
        Tab::Sessions,
        "a hidden source selection must not own Escape"
    );
    assert_eq!(surface.app.source_claim, Some(0));
    assert!(surface.app.backend.is_none());
}

#[test]
fn help_content_fits_a_narrow_enlarged_workspace_through_the_full_document() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.app.active_tab = Tab::Help;
    surface.settle();
    let mut saw_storage = false;
    for _ in 0..30 {
        let viewport = surface.ctx.content_rect();
        for (label, rect) in &surface.labels {
            assert!(
                rect.right() <= viewport.right() + 0.5,
                "{label:?} extends beyond {viewport:?}: {rect:?}"
            );
            saw_storage |= label.contains("Browser decisions and uploaded documents");
        }
        surface.scroll(-180.0);
    }
    assert!(saw_storage, "the end of Help must remain reachable");
    assert!(surface.app.backend.is_none());
}

#[test]
fn run_details_keep_missing_measurements_unknown_and_raw_copy_exact() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.seed_statement();
    surface.app.active_tab = Tab::Diagnostics;
    surface.app.turns[0].diagnostics =
        serde_json::json!({"request_id":"synthetic".repeat(40),"usage":{"prompt_tokens":0}});
    surface.click("1 · Opening · Complete");
    let mut saw_missing_cost = false;
    let mut saw_recorded_zero = false;
    for _ in 0..16 {
        let viewport = surface.ctx.content_rect();
        for (label, rect) in &surface.labels {
            assert!(
                rect.right() <= viewport.right() + 0.5,
                "{label:?} extends beyond {viewport:?}: {rect:?}"
            );
            assert!(
                !label.contains("null"),
                "missing measurements need readable meaning: {label}"
            );
            saw_missing_cost |= label.contains("Cost: Not recorded");
            saw_recorded_zero |= label == "Input tokens: 0";
        }
        surface.scroll(-180.0);
    }
    assert!(saw_missing_cost);
    assert!(saw_recorded_zero, "a recorded zero is not a missing value");
    surface.scroll(10000.0);
    surface.click("Copy run details");
    let bundle: serde_json::Value = serde_json::from_str(surface.copied.last().unwrap()).unwrap();
    assert_eq!(
        bundle["stages"][0]["diagnostics"],
        surface.app.turns[0].diagnostics
    );
    assert_eq!(
        bundle["stages"][0]["diagnostics"]["usage"]["prompt_tokens"],
        0
    );
    assert!(surface.app.backend.is_none());
}

#[test]
fn expanded_raw_request_details_wrap_long_ids_and_keep_cost_attribution() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.seed_statement();
    surface.app.active_tab = Tab::Diagnostics;
    let request_id = "synthetic-id-".repeat(40);
    surface.app.turns[0].diagnostics = serde_json::json!({"request_id":request_id,"cost":{"amount":0,"currency":"USD","source":"provider-reported"}});
    surface.click("1 · Opening · Complete");
    let mut seen_cost = false;
    let mut seen_currency = false;
    let mut seen_source = false;
    for _ in 0..16 {
        for (label, _) in &surface.labels {
            seen_cost |= label == "Cost: 0";
            seen_currency |= label == "Currency: USD";
            seen_source |= label == "Cost source: provider-reported";
        }
        surface.scroll(-120.0);
    }
    assert!(seen_cost && seen_currency && seen_source);
    surface.scroll(10000.0);
    surface.click("Request details");
    let mut saw_exact_id = false;
    for _ in 0..24 {
        let viewport = surface.ctx.content_rect();
        for (label, rect) in &surface.labels {
            assert!(
                rect.right() <= viewport.right() + 0.5,
                "{label:?} extends beyond {viewport:?}: {rect:?}"
            );
            saw_exact_id |= label.contains(&request_id);
        }
        surface.scroll(-180.0);
    }
    assert!(
        saw_exact_id,
        "wrapping must not truncate the diagnostic record"
    );
    assert!(surface.app.backend.is_none());
}

#[test]
fn completed_decision_leads_with_the_question_and_discloses_repeat_actions() {
    for (width, zoom) in [(1280.0, 1.0), (320.0, 1.5)] {
        let mut surface = Surface::new();
        surface.viewport = egui::vec2(width, 900.0);
        surface.ctx.set_zoom_factor(zoom);
        surface.seed_statement();
        surface.app.active_tab = Tab::Brief;
        surface.app.last_config = Some(RunConfig {
            question: surface.app.current_question.clone(),
            rounds: 1,
            dry_run: true,
            roles: council_ui::roles::defaults(),
            ..Default::default()
        });
        surface.settle();
        assert!(
            !surface
                .labels
                .iter()
                .any(|(text, _)| text == "Review a new run" || text == "Run details")
        );
        let question = surface
            .labels
            .iter()
            .find(|(text, _)| text == "Should we choose A or B?")
            .unwrap()
            .1;
        let actions = surface
            .labels
            .iter()
            .find(|(text, _)| text == "Decision actions")
            .unwrap()
            .1;
        assert!(question.bottom() <= actions.top());
        surface.click("Decision actions");
        surface.click("Review a new run");
        assert!(!egui::Popup::is_any_open(&surface.ctx));
        let review = surface.app.execution_review.as_ref().unwrap();
        assert_eq!(review.retained(), 0);
        assert_eq!(review.remaining(), 6);
        assert!(surface.app.backend.is_none());
        surface.click("Cancel");
        surface.click("Decision actions");
        surface.click("Run details");
        assert_eq!(surface.app.active_tab, Tab::Diagnostics);
        assert!(!egui::Popup::is_any_open(&surface.ctx));
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn stopping_remains_directly_available_while_the_decision_runs() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.seed_statement();
    surface.app.active_tab = Tab::Debate;
    surface
        .app
        .start_mock_run("Synthetic progress fixture".into());
    surface.click("Stop run");
    assert!(!surface.app.is_running);
    assert!(surface.app.stopped_by_user);
    assert_eq!(surface.app.turns[0].status, TurnStatus::Complete);
    assert!(surface.app.backend.is_none());
}

#[test]
fn brief_copy_and_claim_inspection_do_not_require_scrolling_past_long_rationale() {
    for (width, zoom) in [(1280.0, 1.0), (320.0, 1.5)] {
        let mut surface = Surface::new();
        surface.viewport = egui::vec2(width, 900.0);
        surface.ctx.set_zoom_factor(zoom);
        surface.seed_statement();
        surface.app.active_tab = Tab::Brief;
        surface.app.brief_text = format!(
            "# Recommendation\n\nChoose A conditionally.\n\n# Tradeoffs\n\n{}\n\nLast verbatim paragraph.",
            "Long rationale retained in full.\n\n".repeat(200)
        );
        surface.settle();
        if width > 600.0 {
            assert!(
                surface
                    .labels
                    .iter()
                    .any(|(text, _)| text == "Copy qualified brief")
            );
        }
        surface.click("Copy qualified brief");
        assert_eq!(
            surface.copied.last().unwrap(),
            &council_ui::brief::qualified_export(&surface.app.brief_text, false)
        );
        assert!(
            surface
                .copied
                .last()
                .unwrap()
                .contains("Last verbatim paragraph.")
        );
        surface.click("Inspect claims & sources");
        assert_eq!(surface.app.active_tab, Tab::Claims);
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn duplicate_document_names_have_independent_inspection_and_disclosed_storage_paths() {
    use council_ui::{
        execution::{ExecutionReview, ReviewSource},
        preflight::{DocumentCheck, DocumentSnapshot, PreflightReport},
    };
    for (width, zoom) in [(1280.0, 1.0), (320.0, 1.5)] {
        let mut surface = Surface::new();
        surface.viewport = egui::vec2(width, 900.0);
        surface.ctx.set_zoom_factor(zoom);
        surface.seed_statement();
        let name = "Planning café & costs #1 + 100%.txt";
        let paths: Vec<_> = ["first-id", "second-id"]
            .iter()
            .map(|id| format!("/server/.council-context/{id}/{name}"))
            .collect();
        surface.app.execution_review = Some(ExecutionReview {
            source: ReviewSource::Composer,
            config: RunConfig {
                question: "Synthetic duplicate document review".into(),
                rounds: 1,
                dry_run: true,
                context_files: paths.clone(),
                ..Default::default()
            },
            message: String::new(),
            preflight: Some(PreflightReport {
                ready: true,
                documents: ["First synthetic material.", "Second synthetic material."]
                    .into_iter()
                    .map(|text| DocumentCheck {
                        snapshot: DocumentSnapshot {
                            name: name.into(),
                            text: text.into(),
                            original_characters: text.chars().count(),
                            ..Default::default()
                        },
                        error: String::new(),
                    })
                    .collect(),
                ..Default::default()
            }),
        });
        surface.settle();
        assert!(
            !surface
                .labels
                .iter()
                .any(|(label, _)| label.contains(".council-context"))
        );
        surface.click(&format!("Inspect included text — Document 1 · {name}"));
        surface.reveal("First synthetic material.");
        surface.click(&format!("Inspect included text — Document 2 · {name}"));
        surface.reveal("Second synthetic material.");
        surface.scroll(10000.0);
        surface.reveal("First synthetic material.");
        surface.scroll(10000.0);
        surface.click("Document storage references");
        surface.reveal(&format!("Document 1: {}", paths[0]));
        surface.reveal(&format!("Document 2: {}", paths[1]));
        for (label, rect) in &surface.labels {
            assert!(
                rect.right() <= surface.ctx.content_rect().right() + 0.5,
                "{label:?} exceeds the viewport: {rect:?}"
            );
        }
        surface.click("Cancel");
        assert!(surface.app.execution_review.is_none());
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn same_named_source_documents_have_distinct_material_controls() {
    use council_ui::preflight::DocumentSnapshot;
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.seed_statement();
    let name = "Planning café & costs #1 + 100%.txt";
    surface.app.turns[0].text =
        "C1: [SOURCE EXCERPT MATCHED] Shared supplied text | source: \"synthetic material\"".into();
    surface.app.claims = council_ui::claims::extract_claims(&surface.app.turns);
    surface.app.last_config = Some(RunConfig {
        context_snapshot: Some(
            ["First synthetic material.", "Second synthetic material."]
                .into_iter()
                .map(|text| DocumentSnapshot {
                    name: name.into(),
                    text: text.into(),
                    ..Default::default()
                })
                .collect(),
        ),
        ..Default::default()
    });
    assert_eq!(surface.app.claims.len(), 1);
    surface.settle();

    surface.reveal("Inspect statement, challenges and response");
    for (label, rect) in &surface.labels {
        assert!(
            rect.left() >= -0.5 && rect.right() <= surface.ctx.content_rect().right() + 0.5,
            "{label:?} exceeds the viewport: {rect:?}"
        );
    }
    surface.click("Inspect statement, challenges and response");
    surface.click(&format!("Inspect supplied material — Document 1 · {name}"));
    surface.reveal("First synthetic material.");
    surface.click(&format!("Inspect supplied material — Document 2 · {name}"));
    surface.reveal("Second synthetic material.");
    surface.scroll(10000.0);
    surface.reveal("First synthetic material.");
    assert!(surface.app.backend.is_none());
}

#[test]
fn same_named_documents_remain_removable_in_a_narrow_composer() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.app.composer_open = true;
    surface.app.composer_state.question = "Synthetic document naming check".into();
    let name = "Planning café & costs #1 + 100%.txt";
    let first = format!("/server/.council-context/first-id/{name}");
    let second = format!("/server/.council-context/second-id/{name}");
    surface.app.composer_state.context_files = vec![first, second.clone()];
    surface.reveal("Remove");
    for (label, rect) in &surface.labels {
        assert!(
            rect.left() >= -0.5 && rect.right() <= surface.ctx.content_rect().right() + 0.5,
            "{label:?} exceeds the viewport: {rect:?}"
        );
    }
    surface.click("Remove");
    assert_eq!(surface.app.composer_state.context_files, [second]);
    surface.click("Cancel");
    assert!(surface.app.backend.is_none());
}

#[test]
fn source_return_restores_its_opener_after_the_claim_layout_changes_width() {
    for tab in [Tab::Claims, Tab::Brief] {
        let mut surface = Surface::new();
        surface.viewport = egui::vec2(320.0, 900.0);
        surface.ctx.set_zoom_factor(1.5);
        surface.seed_statement();
        surface.app.active_tab = tab;
        if tab == Tab::Brief {
            surface.click("Trace a claim to its argument");
            let claim = &surface.app.claims[0];
            let label = format!(
                "{} · stage {} · {}",
                claim.id, claim.turn_index, claim.author_model
            );
            surface.click(&label);
        } else {
            surface.click("Inspect statement, challenges and response");
        }
        let opener = surface.app.source_return_focus.unwrap();
        surface.viewport = egui::vec2(1280.0, 900.0);
        surface.settle();
        surface.click(if tab == Tab::Brief {
            "< Back to Decision brief"
        } else {
            "< Back to Claims & sources"
        });
        assert_eq!(surface.app.active_tab, tab);
        assert_eq!(surface.app.selected_claim, 0);
        assert_eq!(surface.ctx.memory(egui::Memory::focused), Some(opener));
        assert!(surface.app.backend.is_none());
    }
}

#[test]
fn fresh_keyboard_navigation_overrides_a_queued_source_return_focus() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface.click("Inspect statement, challenges and response");
    let opener = surface.app.source_return_focus.unwrap();
    surface.app.return_from_source();
    assert_eq!(surface.app.pending_focus, Some(opener));
    surface.press(egui::Key::ArrowDown);
    assert!(surface.app.pending_focus.is_none());
    assert_ne!(surface.ctx.memory(egui::Memory::focused), Some(opener));
    assert_eq!(surface.app.active_tab, Tab::Claims);
    assert!(surface.app.backend.is_none());
}

#[test]
fn doubled_text_preserves_narrow_navigation_reading_and_setup_actions() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(2.0);
    surface.seed_statement();
    surface.settle();
    surface.click("Navigate");
    surface.click("Help");
    let mut saw_storage = false;
    for _ in 0..100 {
        for (label, rect) in &surface.labels {
            assert!(
                rect.right() <= surface.ctx.content_rect().right() + 0.5,
                "{label:?} overflows doubled text: {rect:?}"
            );
            saw_storage |= label.contains("Browser decisions and uploaded documents");
        }
        surface.scroll(-180.0);
    }
    assert!(saw_storage, "Help storage guidance must remain reachable");
    surface.press(egui::Key::Num2);
    surface.click("Copy qualified brief");
    assert!(
        surface
            .copied
            .last()
            .unwrap()
            .contains("Fixture recommendation.")
    );
    surface.press(egui::Key::N);
    surface.app.composer_state.question = "Doubled-text decision?".into();
    surface.settle();
    surface.click("Review run");
    surface.click("Cancel");
    assert!(!surface.app.is_running);
}

#[test]
fn launch_error_dismissal_keeps_the_original_brief_and_does_not_dispatch() {
    let mut surface = Surface::new();
    surface.viewport = egui::vec2(320.0, 900.0);
    surface.ctx.set_zoom_factor(1.5);
    surface.seed_statement();
    surface.app.active_tab = Tab::Brief;
    surface.app.launch_failure = Some(council_ui::execution::LaunchFailure {
        question: "A separately reviewed replacement question".into(),
        message: "Synthetic engine launch failure".into(),
    });
    surface.settle();
    surface.reveal("New run could not start");
    surface.click("Attempted decision question");
    surface.reveal("A separately reviewed replacement question");
    surface.press(egui::Key::Num2);
    surface.click("Dismiss launch error");
    assert!(surface.app.launch_failure.is_none());
    surface.click("Copy qualified brief");
    assert!(
        surface
            .copied
            .last()
            .unwrap()
            .contains("Fixture recommendation.")
    );
    assert!(surface.app.backend.is_none());
    assert_eq!(surface.app.current_question, "Should we choose A or B?");
}

#[test]
fn replacement_review_explains_that_unsaved_work_will_be_replaced() {
    let mut surface = Surface::new();
    surface.seed_statement();
    surface
        .app
        .start_run("Replacement question".into(), 1, true);
    surface.settle();
    surface.reveal("Starting replaces the current decision in this window. Only confirmed saved output can be reopened from history. Cancel and copy any unsaved work before starting.");
    surface.click("Cancel");
    assert!(surface.app.backend.is_none());
    assert_eq!(
        surface.app.brief_text,
        "# Recommendation\n\nFixture recommendation."
    );
}

#[test]
fn first_launch_failure_offers_recovery_without_inventing_a_previous_decision() {
    let mut surface = Surface::new();
    surface.app.launch_failure = Some(council_ui::execution::LaunchFailure {
        question: "First attempted decision".into(),
        message: "Synthetic startup error".into(),
    });
    surface.settle();
    surface.reveal("The run did not open. Review the setup before trying again. No retry happens automatically.");
    surface.click("Dismiss launch error");
    surface.click("Start a decision");
    assert!(surface.app.composer_open);
    assert!(!surface.app.has_workspace());
    assert!(surface.app.backend.is_none());
}
