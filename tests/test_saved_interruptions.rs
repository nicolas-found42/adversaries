//! Recorded interruption, file-open failure and unfinished work are distinct.
mod common;
use council_ui::{
    app::App,
    model::TurnStatus,
    session_loader::{load_session, scan_sessions},
};
use serde_json::json;
use std::{fs, path::Path};

fn saved(dir: &Path, name: &str, failed: Option<&str>) -> String {
    let mut data = json!({"question":"Synthetic A or B?", "brief":"", "turns":[
        {"index":1,"model":"Opening provider / model-id","role":"Opening","text":"C1: Complete.","status":"complete"}
    ]});
    if let Some(reason) = failed {
        data["failed_turn"] = json!({"index":2,"model":"Critic provider / exact-id","role":"Red team 1","text":"Partial response.","status":"failed","diagnostics":{"message":reason}});
    }
    let path = dir.join(format!("{name}.json"));
    fs::write(&path, data.to_string()).unwrap();
    path.to_string_lossy().into()
}

#[test]
fn history_and_replay_preserve_the_recorded_failure_reason_without_running() {
    let dir = tempfile::tempdir().unwrap();
    let path = saved(
        dir.path(),
        "failed",
        Some("Synthetic provider timeout after receiving partial output."),
    );
    let summary = scan_sessions(dir.path()).remove(0);
    let interruption = summary.interruption.unwrap();
    assert_eq!(interruption.stage_index, 2);
    assert_eq!(interruption.model, "Critic provider / exact-id");
    assert_eq!(
        interruption.reason.as_deref(),
        Some("Synthetic provider timeout after receiving partial output.")
    );
    let mut app = App::new(false, None, true);
    app.load_session_by_path(&path);
    common::finish_storage(&mut app);
    assert!(app.error.as_deref().unwrap().contains("Saved stage 2"));
    assert!(
        app.error
            .as_deref()
            .unwrap()
            .contains("Synthetic provider timeout")
    );
    assert!(!app.is_running);
    assert!(app.backend.is_none());
    assert_eq!(app.turns[1].text, "Partial response.");
}

#[test]
fn ordinary_unfinished_work_does_not_inherit_an_old_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = saved(dir.path(), "unfinished", None);
    assert!(scan_sessions(dir.path())[0].interruption.is_none());
    let mut app = App::new(false, None, true);
    app.error = Some("A previous run's failure.".into());
    app.load_session_by_path(&path);
    common::finish_storage(&mut app);
    assert!(app.error.is_none());
    assert!(app.needs_brief());
    assert!(app.backend.is_none());
}

#[test]
fn restored_running_output_is_incomplete_and_does_not_invent_a_failure_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("abandoned.json");
    fs::write(
        &path,
        json!({"question":"Synthetic?","brief":"","turns":[
            {"index":1,"role":"Opening","model":"Model","text":"Partial output.","status":"running"}
        ]})
        .to_string(),
    )
    .unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert_eq!(detail.turns[0].status, TurnStatus::Incomplete);
    let interruption = scan_sessions(dir.path())[0].interruption.clone().unwrap();
    assert!(interruption.reason.is_none());
    let mut app = App::new(false, None, true);
    app.load_session_by_path(path.to_str().unwrap());
    common::finish_storage(&mut app);
    assert!(
        app.error
            .as_deref()
            .unwrap()
            .contains("No failure reason was recorded")
    );
    assert!(!app.is_running);
    assert!(app.backend.is_none());
}

#[test]
fn history_exposes_interruption_reason_and_keeps_partial_work_openable() {
    let dir = tempfile::tempdir().unwrap();
    saved(
        dir.path(),
        "failed",
        Some("Synthetic timeout, not a live incident."),
    );
    let path = dir.path().join("failed.json");
    let mut data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    data["turns"] = json!([]);
    fs::write(path, data.to_string()).unwrap();
    let mut app = App::new(false, None, true);
    app.active_tab = council_ui::model::Tab::Sessions;
    app.sessions = scan_sessions(dir.path());
    let text = history_semantics(&mut app, 620.0);
    assert!(text.contains("Interrupted · no brief"));
    assert!(text.contains("Synthetic timeout, not a live incident."));
    assert!(text.contains("Opening saved work does not retry this stage"));
    assert!(text.contains("Open selected decision"));
    assert!(text.contains("No completed arguments were saved"));
    assert!(!text.contains("nothing to read"));
}

fn history_semantics(app: &mut App, width: f32) -> String {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut tree = String::new();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            council_ui::ui::render(ui, app);
            tree = serde_json::to_string(&council_ui::accessibility::snapshot(&ctx)).unwrap();
        },
    );
    output.textures_delta.clear();
    tree
}

#[test]
fn reading_completed_work_leaves_partial_source_inspection_without_retrying() {
    let dir = tempfile::tempdir().unwrap();
    let path = saved(dir.path(), "partial", Some("Synthetic timeout."));
    let mut app = App::new(false, None, true);
    app.load_session_by_path(&path);
    common::finish_storage(&mut app);
    app.selected_turn = 1;
    app.source_claim = Some(0);
    app.follow_live = true;
    app.read_completed_arguments();
    assert_eq!(app.selected_turn, 0);
    assert!(app.source_claim.is_none());
    assert!(!app.follow_live);
    assert!(app.status_message.contains("No model request was started"));
    assert!(app.backend.is_none());
}

#[test]
fn older_history_payloads_without_interruption_metadata_still_deserialize() {
    let summary: council_ui::model::SessionSummary = serde_json::from_value(json!({
        "file_name":"legacy.md","path_str":"reports/legacy.md","created_at":"unknown",
        "question":"Synthetic?","rounds":1,"turn_count":2,"has_brief":false
    }))
    .unwrap();
    assert!(summary.interruption.is_none());
}

#[test]
fn argument_copy_preserves_partial_status_and_attribution_outside_the_ui() {
    let mut turn =
        council_ui::model::TurnState::new(2, "Provider / exact-id".into(), "Critique".into());
    turn.status = TurnStatus::Incomplete;
    turn.text = "A partial ünicode argument.".into();
    let copy = council_ui::brief::argument_export(&turn, true);
    assert!(copy.contains("Practice example: simulated content."));
    assert!(copy.contains("Incomplete stage output"));
    assert!(copy.contains("Stage 2 · Critique · Provider / exact-id"));
    assert!(copy.ends_with(&turn.text));
    turn.status = TurnStatus::Complete;
    let copy = council_ui::brief::argument_export(&turn, false);
    assert!(copy.contains("Completed stage output"));
    assert!(!copy.contains("Incomplete stage output"));
}

#[test]
fn selected_history_row_exposes_selection_and_readiness_to_assistive_controls() {
    let dir = tempfile::tempdir().unwrap();
    saved(dir.path(), "failed", Some("Synthetic timeout."));
    let mut app = App::new(false, None, true);
    app.active_tab = council_ui::model::Tab::Sessions;
    app.sessions = scan_sessions(dir.path());
    let tree: serde_json::Value =
        serde_json::from_str(&history_semantics(&mut app, 620.0)).unwrap();
    assert!(selected_interrupted_record(&tree));
    assert!(
        !duplicate_row_metadata(&tree),
        "painted metadata must not repeat the row's accessible name"
    );
}

fn duplicate_row_metadata(node: &serde_json::Value) -> bool {
    let duplicate = node["role"] == "Label"
        && node["value"]
            .as_str()
            .is_some_and(|value| value.contains("2 stages · Interrupted"));
    duplicate
        || node["children"]
            .as_array()
            .is_some_and(|children| children.iter().any(duplicate_row_metadata))
}

fn selected_interrupted_record(node: &serde_json::Value) -> bool {
    if node["role"] == "Button"
        && node["toggled"] == true
        && node["label"].as_str().is_some_and(|label| {
            label.contains("Synthetic A or B?") && label.contains("Interrupted · no brief")
        })
    {
        return true;
    }
    node["children"]
        .as_array()
        .is_some_and(|children| children.iter().any(selected_interrupted_record))
}

#[test]
fn reading_completed_arguments_is_disabled_until_an_argument_exists() {
    let mut app = App::new(false, None, true);
    app.current_question = "Synthetic decision?".into();
    app.active_tab = council_ui::model::Tab::Brief;
    app.is_running = true;
    let tree: serde_json::Value =
        serde_json::from_str(&history_semantics(&mut app, 620.0)).unwrap();
    assert!(disabled_completed_reader(&tree));
}

fn disabled_completed_reader(node: &serde_json::Value) -> bool {
    if node["label"] == "Read completed arguments" && node["disabled"] == true {
        return true;
    }
    node["children"]
        .as_array()
        .is_some_and(|children| children.iter().any(disabled_completed_reader))
}
