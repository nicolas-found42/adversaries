use super::{App, SessionDetail, Tab};
use std::sync::mpsc::{Sender, channel};

pub(super) fn finish_open(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.session_opening() {
        app.tick();
        assert!(
            std::time::Instant::now() < deadline,
            "native fixture did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn readable_app() -> App {
    let mut app = App::new(false, None, true);
    app.current_question = "Original decision".into();
    app.brief_text = "Original readable recommendation.".into();
    app
}

fn pending(app: &mut App) -> Sender<Option<SessionDetail>> {
    let (sender, receiver) = channel();
    app.begin_session_open("target.json", receiver);
    sender
}

fn detail() -> SessionDetail {
    SessionDetail {
        question: "Loaded decision".into(),
        brief: "Loaded recommendation.".into(),
        turns: Vec::new(),
        run_config: None,
        diagnostics: serde_json::Value::Null,
    }
}

#[test]
fn a_waiting_file_read_keeps_the_current_workspace_available_and_can_be_cancelled() {
    let mut app = readable_app();
    let sender = pending(&mut app);
    app.tick();
    assert!(app.session_opening() && app.needs_poll());
    assert_eq!(app.current_question, "Original decision");
    assert_eq!(app.brief_text, "Original readable recommendation.");
    app.cancel_session_open();
    assert!(sender.send(Some(detail())).is_err());
    app.tick();
    assert!(!app.session_opening());
    assert_eq!(app.current_question, "Original decision");
    assert!(app.backend.is_none());
}

#[test]
fn navigation_away_discards_a_late_file_result() {
    let mut app = readable_app();
    let sender = pending(&mut app);
    app.active_tab = Tab::Help;
    sender.send(Some(detail())).unwrap();
    app.tick();
    assert!(!app.session_opening());
    assert_eq!(app.active_tab, Tab::Help);
    assert_eq!(app.current_question, "Original decision");
}

#[test]
fn a_new_composer_is_not_replaced_by_an_earlier_file_read() {
    let mut app = readable_app();
    let sender = pending(&mut app);
    app.composer_open = true;
    app.composer_state.question = "New draft".into();
    sender.send(Some(detail())).unwrap();
    app.tick();
    assert!(!app.session_opening());
    assert_eq!(app.composer_state.question, "New draft");
    assert_eq!(app.current_question, "Original decision");
}

#[test]
fn a_disconnected_file_reader_reports_failure_without_replacing_existing_work() {
    let mut app = readable_app();
    drop(pending(&mut app));
    app.tick();
    assert!(app.load_failed());
    assert_eq!(app.current_question, "Original decision");
    assert_eq!(app.brief_text, "Original readable recommendation.");
    assert!(app.backend.is_none());
}

#[test]
fn successful_file_delivery_applies_the_record_and_key_only_once() {
    let mut app = readable_app();
    app.session_key = "original".into();
    let sender = pending(&mut app);
    assert_eq!(app.session_key, "original");
    sender.send(Some(detail())).unwrap();
    app.tick();
    let generation = app.workspace_generation;
    assert_eq!(app.current_question, "Loaded decision");
    assert_eq!(app.session_key, "target");
    app.tick();
    assert_eq!(app.workspace_generation, generation);
    assert!(app.backend.is_none());
}

#[test]
fn repeated_refreshes_leave_one_existing_list_request_and_queue_one_followup() {
    let mut app = readable_app();
    let (sender, receiver) = channel();
    app.pending_sessions = Some(receiver);
    app.refresh_sessions();
    app.refresh_sessions();
    assert!(app.history_refresh_requested);
    sender.send(Ok(Vec::new())).unwrap();
    assert!(app.pending_sessions.as_ref().unwrap().try_recv().is_ok());
    assert_eq!(app.current_question, "Original decision");
}

#[test]
fn a_new_execution_review_is_not_replaced_by_an_earlier_file_read() {
    let mut app = readable_app();
    let sender = pending(&mut app);
    app.demo_mode = true;
    app.start_run("New reviewed plan".into(), 1, true);
    sender.send(Some(detail())).unwrap();
    app.tick();
    assert!(!app.session_opening());
    assert_eq!(
        app.execution_review.as_ref().unwrap().config.question,
        "New reviewed plan"
    );
    assert_eq!(app.current_question, "Original decision");
    assert!(app.backend.is_none());
}

#[test]
fn a_disconnected_history_reader_reports_an_error_without_replacing_the_workspace() {
    let mut app = readable_app();
    let (sender, receiver) = channel();
    app.pending_sessions = Some(receiver);
    drop(sender);
    app.tick();
    assert!(!app.history_loading());
    assert!(
        app.history_error
            .as_deref()
            .unwrap()
            .contains("ended without a result")
    );
    assert_eq!(app.current_question, "Original decision");
    assert_eq!(app.brief_text, "Original readable recommendation.");
    assert!(app.backend.is_none());
}

fn storage_frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 900.0) / ctx.zoom_factor(),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            crate::ui::render(ui, app);
        },
    )
}

#[test]
fn a_long_pending_question_keeps_cancel_in_the_first_narrow_enlarged_viewport() {
    let ctx = egui::Context::default();
    ctx.set_zoom_factor(1.5);
    let mut app = readable_app();
    app.active_tab = Tab::Sessions;
    app.sessions.push(super::SessionSummary {
        file_name: "target.json".into(),
        path_str: "target.json".into(),
        created_at: "2026-11-02T16:00:00".into(),
        question: "A long synthetic question and supporting context. ".repeat(400),
        rounds: 1,
        turn_count: 1,
        has_brief: true,
        interruption: None,
        dry_run: Some(true),
    });
    let _sender = pending(&mut app);
    for _ in 0..4 {
        let mut output = storage_frame(&ctx, &mut app, Vec::new());
        let cancel = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == "Cancel opening" => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
            .unwrap();
        assert!(
            ctx.content_rect().contains_rect(cancel),
            "Cancel is outside the viewport: {cancel:?}"
        );
        output.textures_delta.clear();
    }
    app.cancel_session_open();
    assert_eq!(app.current_question, "Original decision");
    assert!(app.backend.is_none());
}

fn field_id(node: &serde_json::Value, placeholder: &str) -> Option<u64> {
    if node["placeholder"] == placeholder && node["set_value"] == true {
        return node["id"].as_str()?.parse().ok();
    }
    node["children"]
        .as_array()?
        .iter()
        .find_map(|child| field_id(child, placeholder))
}

fn semantic_frame(
    ctx: &egui::Context,
    app: &mut App,
    events: Vec<egui::Event>,
) -> serde_json::Value {
    let mut tree = serde_json::Value::Null;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(620.0, 1200.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            crate::ui::render(ui, app);
            tree = serde_json::to_value(crate::accessibility::snapshot(ctx)).unwrap();
        },
    );
    output.textures_delta.clear();
    tree
}

fn set_field(id: u64, text: &str) -> egui::Event {
    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action: egui::accesskit::Action::SetValue,
        target_tree: egui::accesskit::TreeId::ROOT,
        target_node: id.into(),
        data: Some(egui::accesskit::ActionData::Value(text.into())),
    })
}

#[test]
fn queued_history_filter_input_survives_pending_notice_removal() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = readable_app();
    app.active_tab = Tab::Sessions;
    app.sessions.push(super::SessionSummary {
        file_name: "target.json".into(),
        path_str: "target.json".into(),
        created_at: "2026-11-02T16:00:00".into(),
        question: "Target fixture".into(),
        rounds: 1,
        turn_count: 1,
        has_brief: true,
        interruption: None,
        dry_run: Some(true),
    });
    let _sender = pending(&mut app);
    let tree = semantic_frame(&ctx, &mut app, Vec::new());
    let before = field_id(&tree, "Find a saved decision").unwrap();
    app.cancel_session_open();
    let tree = semantic_frame(
        &ctx,
        &mut app,
        vec![set_field(before, "New history filter")],
    );
    assert_eq!(app.history_search, "New history filter");
    assert_eq!(field_id(&tree, "Find a saved decision"), Some(before));
}

#[test]
fn queued_claim_filter_input_survives_a_new_recovery_notice() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = readable_app();
    app.active_tab = Tab::Claims;
    let tree = semantic_frame(&ctx, &mut app, Vec::new());
    let before = field_id(&tree, "Find a claim, author, or debate outcome").unwrap();
    app.error = Some("Synthetic interrupted stage.".into());
    let tree = semantic_frame(&ctx, &mut app, vec![set_field(before, "New claim filter")]);
    assert_eq!(app.claim_search, "New claim filter");
    assert_eq!(
        field_id(&tree, "Find a claim, author, or debate outcome"),
        Some(before)
    );
}
