//! A saved record's history status and opened reader must agree.
mod common;
use council_ui::{
    app::App,
    model::{Tab, TurnStatus},
    session_loader::{load_session, scan_sessions},
};
use serde_json::json;
use std::{fs, path::Path};

fn report_pair(dir: &Path, name: &str, data: &serde_json::Value) -> String {
    fs::write(
        dir.join(format!("{name}.md")),
        "# Decision in progress\n\n**Question:** A or B?\n",
    )
    .unwrap();
    let path = dir.join(format!("{name}.json"));
    fs::write(&path, serde_json::to_string(data).unwrap()).unwrap();
    path.to_string_lossy().into()
}

#[test]
fn whitespace_brief_is_unfinished_in_history_and_when_opened() {
    let dir = tempfile::tempdir().unwrap();
    let path = report_pair(
        dir.path(),
        "whitespace",
        &json!({
            "question": "A or B?", "brief": " \n\t\u{2003}",
            "turns": [{"index":1,"role":"Opening","model":"Model","text":"C1: A completed argument."}]
        }),
    );
    let sessions = scan_sessions(dir.path());
    assert!(!sessions[0].has_brief);
    assert_eq!(sessions[0].turn_count, 1);
    let mut app = App::new(false, None, true);
    app.load_session_by_path(&path);
    common::finish_storage(&mut app);
    assert_eq!(app.active_tab, Tab::Debate);
    assert!(app.needs_brief());
    assert_eq!(app.turns.len(), 1);
    assert!(app.status_message.contains("no brief"));
    assert!(app.backend.is_none());
}

#[test]
fn markdown_checkpoint_metadata_is_not_a_generated_brief() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checkpoint.md");
    fs::write(&path, "# Decision in progress\n\n**Question:** A or B?\n").unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert!(detail.brief.is_empty());
    let mut app = App::new(false, None, true);
    app.load_session_by_path(path.to_str().unwrap());
    common::finish_storage(&mut app);
    assert!(
        app.needs_brief(),
        "a zero-argument checkpoint is unfinished too"
    );
    assert!(app.backend.is_none());
}

#[test]
fn markdown_title_without_a_body_is_not_ready() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty-brief.md");
    fs::write(&path, "# Decision Brief\n\n---\n# Appendix: Full adversarial debate\n\n**Question:** A or B?\n\n## Turn 1 — Model: Opening\n\nC1: Saved work.\n").unwrap();
    assert!(!scan_sessions(dir.path())[0].has_brief);
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert!(detail.brief.is_empty());
    assert_eq!(detail.turns[0].text, "C1: Saved work.");
}

#[test]
fn explicitly_failed_brief_is_preserved_as_partial_instead_of_ready() {
    let dir = tempfile::tempdir().unwrap();
    let path = report_pair(
        dir.path(),
        "failed-brief",
        &json!({
            "question":"A or B?", "brief":"Partial answer, not finished.",
            "turns":[{"index":1,"role":"Opening","model":"Model","text":"C1: Saved work."}],
            "brief_turn":{"index":2,"role":"Brief","model":"Writer","text":"Partial answer, not finished.","status":"failed"}
        }),
    );
    assert!(!scan_sessions(dir.path())[0].has_brief);
    let detail = load_session(&path).unwrap();
    assert!(detail.brief.is_empty());
    assert_eq!(detail.turns[1].text, "Partial answer, not finished.");
    assert_eq!(detail.turns[1].status, TurnStatus::Failed);
}

#[test]
fn json_only_session_is_discoverable_without_duplicate_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let path = report_pair(
        dir.path(),
        "recovered",
        &json!({"question":"A or B?","brief":"Choose A.","turns":[]}),
    );
    assert_eq!(scan_sessions(dir.path()).len(), 1);
    fs::remove_file(dir.path().join("recovered.md")).unwrap();
    fs::write(dir.path().join("unrelated.json"), "{\"events\":[]}").unwrap();
    let sessions = scan_sessions(dir.path());
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].path_str, path);
    assert!(sessions[0].has_brief);
}

#[test]
fn markdown_fallback_preserves_an_unstructured_generated_brief() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unstructured.md");
    fs::write(&path, "Choose A, with conditions.\n\n---\n# Appendix: Full adversarial debate\n\n**Question:** A or B?\n\n## Turn 1 — Model: Opening\n\nC1: Saved work.\n").unwrap();
    assert!(scan_sessions(dir.path())[0].has_brief);
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert_eq!(detail.brief, "Choose A, with conditions.");
    assert_eq!(detail.turns[0].role, "Opening");
    assert_eq!(detail.turns[0].model, "Model");
}

#[test]
fn replay_preserves_recorded_brief_identity_and_missing_question_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let path = report_pair(
        dir.path(),
        "identity",
        &json!({
            "brief":"Choose A.", "turns":[],
            "brief_turn":{"index":4,"model":"Recorded provider / exact-model-id","role":"Brief","text":"Choose A.","seconds":1.25,"status":"complete"}
        }),
    );
    let mut app = App::new(false, None, true);
    app.load_session_by_path(&path);
    common::finish_storage(&mut app);
    assert!(
        app.current_question.is_empty(),
        "a missing question must not be replaced with invented input"
    );
    assert!(app.has_workspace());
    assert_eq!(app.active_tab, Tab::Brief);
    assert_eq!(app.turns[0].model, "Recorded provider / exact-model-id");
    assert_eq!(app.turns[0].index, 4);
    assert_eq!(app.turns[0].seconds, 1.25);
    assert!(app.backend.is_none());
}

#[test]
fn legacy_markdown_uses_actual_header_attribution_and_separates_the_brief() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.md");
    fs::write(&path, "**Question:** A or B?\n\n# Decision Brief\n\nChoose A.\n\n## Turn 1: Model / exact-id — Opening\n\nC1: Saved work.\n").unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert_eq!(detail.brief, "# Decision Brief\n\nChoose A.");
    assert_eq!(detail.turns[0].model, "Model / exact-id");
    assert_eq!(detail.turns[0].role, "Opening");
    assert_eq!(detail.turns[0].text, "C1: Saved work.");
    assert!(scan_sessions(dir.path())[0].has_brief);
}

#[test]
fn report_metadata_does_not_become_argument_text_or_replace_the_question() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.md");
    fs::write(&path, "# Decision Brief\n\nChoose A.\n\n**Question:** A quoted example, not the original question.\n\n---\n# Appendix: Full adversarial debate\n\n**Question:** The original question?\n\n## Turn 1 — Model / exact-id: Opening\n\nC1: Saved work.\n\n---\n*AI-generated decision support. Verify facts and figures before acting.*\n").unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert_eq!(detail.question, "The original question?");
    assert_eq!(detail.turns[0].text, "C1: Saved work.");
    assert!(detail.brief.contains("A quoted example"));
}

#[test]
fn title_only_sidecar_is_kept_as_incomplete_text_for_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let path = report_pair(
        dir.path(),
        "title",
        &json!({"question":"A or B?","brief":"# Decision Brief\n\n---","turns":[]}),
    );
    let detail = load_session(&path).unwrap();
    assert!(detail.brief.is_empty());
    assert_eq!(detail.turns[0].status, TurnStatus::Incomplete);
    assert_eq!(detail.turns[0].text, "# Decision Brief\n\n---");
    assert!(!scan_sessions(dir.path())[0].has_brief);
}

#[test]
fn readiness_follows_same_length_in_place_text_changes() {
    let mut app = App::new(false, None, true);
    app.brief_text = "# Title".into();
    assert!(!app.has_brief());
    app.brief_text.replace_range(.., "A claim");
    assert!(app.has_brief());
    app.brief_text.clear();
    assert!(!app.has_brief());
}

#[test]
fn a_failure_record_never_enters_retained_work_as_a_completed_argument() {
    let dir = tempfile::tempdir().unwrap();
    let path = report_pair(
        dir.path(),
        "failed-record",
        &json!({
            "question":"A or B?","brief":"",
            "turns":[{"index":1,"model":"Model","role":"Opening","text":"C1: Complete."}],
            "failed_turn":{"index":2,"model":"Critic","role":"Red team","text":"Partial response.","status":"complete"},
            "run_config":{"question":"A or B?","rounds":1,"dry_run":true,"roles":{},"providers":[],"context_files":[],"max_attempts":1}
        }),
    );
    let mut app = App::new(false, None, true);
    app.load_session_by_path(&path);
    common::finish_storage(&mut app);
    assert_eq!(app.turns[1].status, TurnStatus::Incomplete);
    assert_eq!(app.turns[1].text, "Partial response.");
    app.retry_stage();
    assert_eq!(app.execution_review.as_ref().unwrap().retained(), 1);
    assert!(app.backend.is_none());
}

#[test]
fn a_quoted_appendix_heading_inside_code_does_not_split_the_brief() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("quoted.md");
    fs::write(&path, "Choose A.\n\n```text\n# Appendix: Full adversarial debate\nAn example, not the report boundary.\n```\n\n---\n# Appendix: Full adversarial debate\n\n**Question:** A or B?\n\n## Turn 1 — Provider / model:version: Opening\n\nC1: Saved work.\n").unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert!(
        detail
            .brief
            .contains("An example, not the report boundary.")
    );
    assert_eq!(detail.turns.len(), 1);
    assert_eq!(detail.turns[0].model, "Provider / model:version");
    assert_eq!(detail.turns[0].role, "Opening");
}

#[test]
fn missing_question_has_a_readable_workspace_but_is_not_reused_as_run_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = report_pair(
        dir.path(),
        "questionless",
        &json!({"brief":"Choose A.","turns":[]}),
    );
    let mut app = App::new(false, None, true);
    app.load_session_by_path(&path);
    common::finish_storage(&mut app);
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| council_ui::ui::render(ui, &mut app),
    );
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "Question not recorded")));
    output.textures_delta.clear();
    app.rerun_current();
    assert!(app.composer_open);
    assert!(app.composer_state.question.is_empty());
    assert!(app.backend.is_none());
}

#[test]
fn markdown_replay_preserves_stage_numbers_and_ignores_quoted_stage_headers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stage-boundaries.md");
    let body = "C1: Saved work.\n\n```markdown\n## Turn 5 — Pretend model: Red team\nThis is an example.\n```\n\n> ## Turn 8 — Another example: Red team";
    fs::write(&path, format!("Choose A.\n\n---\n# Appendix: Full adversarial debate\n\n**Question:** A or B?\n\n## Turn 4 — Actual model: Opening\n\n{body}\n\n## Turn 9 — Actual critic: Red team\n\nC2: A critique.\n")).unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert_eq!(detail.turns.len(), 2);
    assert_eq!(detail.turns[0].index, 4);
    assert_eq!(detail.turns[0].text, body);
    assert_eq!(detail.turns[1].index, 9);
    assert_eq!(detail.turns[1].model, "Actual critic");
}
