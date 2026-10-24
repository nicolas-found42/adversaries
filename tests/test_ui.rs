//! Regression tests for the framework-free core: wire types, claim ledger,
//! session persistence, the mock event loop, and the `App` state machine.
//! Rendering (egui) and process-spawning (native) are intentionally untested
//! here: they belong to the live surface, verified by `trunk serve`.

use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use council_ui::app::App;
use council_ui::claims::extract_claims;
use council_ui::model::{BackendEvent, ClaimVerdict, Tab, TurnState, TurnStatus};
use council_ui::session_loader::{load_session, scan_sessions};
use council_ui::transport::mock::MockStream;
use council_ui::transport::{RunConfig, council_args};

fn sample_turns() -> Vec<TurnState> {
    vec![
        TurnState {
            index: 1,
            model: "Mistral".to_string(),
            role: "Opening".to_string(),
            text: "C1: Nairobi offers strong talent.\nC2: Timezone overlap is fine.".to_string(),
            seconds: 3.4,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
        TurnState {
            index: 2,
            model: "OpenRouter".to_string(),
            role: "Red team".to_string(),
            text: "Attacking C1: turnover is high, so I reject the discount.\nC2 maintenance costs look fine.\nR1: Keep leads in London.".to_string(),
            seconds: 2.9,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
        TurnState {
            index: 3,
            model: "Mistral".to_string(),
            role: "Rebuttal".to_string(),
            text: "Conceding C1 as revised: adjust the discount to 50%.".to_string(),
            seconds: 3.1,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
    ]
}

#[test]
fn test_backend_event_serialization_roundtrip() {
    let event = BackendEvent::TurnComplete {
        index: 2,
        model: "OpenRouter".to_string(),
        role: "Red team".to_string(),
        text: "Attacking C1.".to_string(),
        seconds: 2.5,
        diagnostics: serde_json::Value::Null,
    };
    let serialized = serde_json::to_string(&event).expect("serialization failed");
    let deserialized: BackendEvent =
        serde_json::from_str(&serialized).expect("deserialization failed");

    match deserialized {
        BackendEvent::TurnComplete {
            index,
            model,
            text,
            seconds,
            ..
        } => {
            assert_eq!(index, 2);
            assert_eq!(model, "OpenRouter");
            assert_eq!(text, "Attacking C1.");
            assert!((seconds - 2.5).abs() < f64::EPSILON);
        }
        other => panic!("unexpected event variant: {other:?}"),
    }
}

#[test]
fn test_claims_extraction_and_verdict_lifecycle() {
    let claims = extract_claims(&sample_turns());
    assert_eq!(claims.len(), 3);

    let c1 = claims.iter().find(|c| c.id == "C1").expect("C1 missing");
    assert_eq!(
        c1.verdict,
        ClaimVerdict::Conceded,
        "the gerund 'Conceding C1' must register as a concession"
    );
    assert!(c1.attackers.contains(&"OpenRouter".to_string()));

    let c2 = claims.iter().find(|c| c.id == "C2").expect("C2 missing");
    assert_eq!(c2.verdict, ClaimVerdict::Disputed);
    assert!(c2.attackers.contains(&"OpenRouter".to_string()));

    let r1 = claims.iter().find(|c| c.id == "R1").expect("R1 missing");
    assert_eq!(r1.verdict, ClaimVerdict::Open);
    assert!(r1.attackers.is_empty());
}

#[test]
fn test_rebuttal_verdicts_accept_prose_inflections_and_late_claim_ids() {
    let turns = vec![
        TurnState {
            index: 1,
            model: "Mistral".to_string(),
            role: "Opening".to_string(),
            text: "C1: Nairobi talent is cheap.\nC2: Timezones align.\nC4: Recommend Nairobi."
                .to_string(),
            seconds: 1.0,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
        TurnState {
            index: 2,
            model: "OpenRouter".to_string(),
            role: "Red team".to_string(),
            text: "Attacking C1: turnover eats the discount.\nR1: Keep leads in London."
                .to_string(),
            seconds: 1.0,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
        TurnState {
            index: 3,
            model: "Mistral".to_string(),
            role: "Rebuttal".to_string(),
            text: "Conceding R1 in part: two leads stay in London.\nConceding C1 as revised: discount drops to 50%.\nDefending C4: the recommendation stands.\nPartially conceding C2: capex accepted."
                .to_string(),
            seconds: 1.0,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
    ];
    let claims = extract_claims(&turns);
    let verdict_of = |id: &str| {
        claims
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} missing"))
            .verdict
            .clone()
    };
    assert_eq!(
        verdict_of("R1"),
        ClaimVerdict::PartiallyConceded,
        "'Conceding X in part' with the id after the keyword must read as partial"
    );
    assert_eq!(
        verdict_of("C1"),
        ClaimVerdict::Conceded,
        "'Conceding X as revised' must concede"
    );
    assert_eq!(
        verdict_of("C4"),
        ClaimVerdict::Rejected,
        "'Defending X' must reject the attack, leaving the claim standing"
    );
    assert_eq!(
        verdict_of("C2"),
        ClaimVerdict::PartiallyConceded,
        "'Partially conceding X' must read as partial"
    );
}

#[test]
fn test_verdict_matching_uses_claim_id_tokens_not_substrings() {
    let turns = vec![
        TurnState {
            index: 1,
            model: "Mistral".to_string(),
            role: "Opening".to_string(),
            text: "C1: Base rate holds.".to_string(),
            seconds: 1.0,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
        TurnState {
            index: 2,
            model: "OpenRouter".to_string(),
            role: "Red team".to_string(),
            text: "R1: Labels are not evidence.\nR10: Research first, then decide.".to_string(),
            seconds: 1.0,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
        TurnState {
            index: 3,
            model: "Mistral".to_string(),
            role: "Rebuttal".to_string(),
            text: "| **R10** (research first) | **CONCEDE** | Agreed, too vague. |".to_string(),
            seconds: 1.0,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Complete,
        },
    ];
    let claims = extract_claims(&turns);
    let r1 = claims.iter().find(|c| c.id == "R1").expect("R1 missing");
    assert_eq!(
        r1.verdict,
        ClaimVerdict::Open,
        "R10 row must not flip R1 via substring match"
    );
    let r10 = claims.iter().find(|c| c.id == "R10").expect("R10 missing");
    assert_eq!(r10.verdict, ClaimVerdict::Conceded);
}

#[test]
fn test_bold_dash_claim_lines_register() {
    let turns = vec![TurnState {
        index: 1,
        model: "OpenCode-Go".to_string(),
        role: "Red team".to_string(),
        text: "**R10 — Research first is not a decision.**\n| **R11** (labels) | **REJECT** | Holds. |"
            .to_string(),
        seconds: 1.0,
            diagnostics: serde_json::Value::Null,
        status: TurnStatus::Complete,
    }];
    let claims = extract_claims(&turns);
    assert!(
        claims.iter().any(|c| c.id == "R10"),
        "bold em-dash claim lines must register"
    );
    assert!(
        claims.iter().any(|c| c.id == "R11"),
        "table-wrapped claim lines must register"
    );
}

fn write_report(dir: &std::path::Path, name: &str, body: &str) {
    fs::write(dir.join(name), body).expect("fixture write failed");
}

#[test]
fn test_session_scan_from_markdown_and_sidecar() {
    let dir = tempfile::tempdir().expect("tempdir failed");

    write_report(
        dir.path(),
        "2026-10-23_alpha.md",
        "**Question:** Should we expand to Nairobi?\n\n# Decision Brief\n\nUseful detail.\n\n## Turn 1: Mistral — Opening\n\nSome opening text.\n",
    );
    write_report(
        dir.path(),
        "2026-10-22_beta.md",
        "**Question:** Older question?\n\nNo brief here.\n",
    );
    write_report(
        dir.path(),
        "2026-10-24_gamma.json",
        r#"{"question": "JSON question?", "rounds": 2, "created_at": "2026-10-24", "brief": "Gamma brief.", "turns": [{"index": 1, "model": "Mistral", "role": "Opening", "text": "C1: x", "seconds": 1.0}]}"#,
    );
    let sessions = scan_sessions(dir.path());
    assert!(
        (1..=2).contains(&sessions.len()),
        "alpha scans for sure; md-less gamma.json is best-effort"
    );

    let alpha = sessions
        .iter()
        .find(|s| s.file_name == "2026-10-23_alpha.md")
        .expect("alpha session missing");
    assert!(alpha.path_str.ends_with("2026-10-23_alpha.md"));
    assert_eq!(alpha.created_at, "2026-10-23_alph");
    assert_eq!(alpha.question, "Should we expand to Nairobi?");
    assert_eq!(alpha.turn_count, 1);
    assert!(alpha.has_brief);
    if let Some(gamma) = sessions.iter().find(|s| s.file_name.contains("gamma")) {
        assert_eq!(gamma.question, "JSON question?");
        assert_eq!(gamma.turn_count, 2);
        assert!(gamma.has_brief);
    }

    assert!(
        sessions[0].file_name == "2026-10-24_gamma.json"
            || sessions[0].file_name == "2026-10-23_alpha.md",
        "sessions sort newest-first"
    );
}

#[test]
fn test_session_load_from_sidecar_json() {
    let dir = tempfile::tempdir().expect("tempdir failed");
    write_report(
        dir.path(),
        "2026-10-24_gamma.json",
        r#"{"question": "JSON question?", "rounds": 2, "created_at": "2026-10-24", "brief": "Gamma brief.", "turns": [{"index": 1, "model": "Mistral", "role": "Opening", "text": "C1: x", "seconds": 1.0}]}"#,
    );

    let path = dir.path().join("2026-10-24_gamma.json");
    let detail = load_session(&path.to_string_lossy()).expect("sidecar load failed");
    assert_eq!(detail.question, "JSON question?");
    assert_eq!(detail.brief, "Gamma brief.");
    assert_eq!(detail.turns.len(), 2);
    assert_eq!(detail.turns[1].role, "Brief");
    assert_eq!(detail.turns[0].model, "Mistral");
    assert_eq!(detail.turns[0].status, TurnStatus::Complete);
}

#[test]
fn test_mock_stream_emits_full_ladder_in_order() {
    let events = drain_mock_stream();
    let types: Vec<&str> = events.iter().map(event_kind).collect();
    assert_eq!(
        types,
        vec![
            "start",
            "turn_start",
            "turn_complete",
            "turn_start",
            "turn_complete",
            "turn_start",
            "turn_complete",
            "turn_start",
            "turn_complete",
            "turn_start",
            "turn_complete",
            "brief_start",
            "brief_complete",
            "done",
        ]
    );
}

#[test]
fn test_mock_stream_first_event_carries_question() {
    let events = drain_mock_stream();
    match &events[0] {
        BackendEvent::Start { question, .. } => assert_eq!(question, "demo question"),
        other => panic!("first event must be Start, got {other:?}"),
    }
}

fn drain_mock_stream() -> Vec<BackendEvent> {
    let cancelled = AtomicBool::new(false);
    let mut stream = MockStream::new("demo question".to_string(), true);
    let (tx, rx) = channel();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while !stream.is_finished() && std::time::Instant::now() < deadline {
        for event in stream.poll(&cancelled) {
            tx.send(event).expect("send failed");
        }
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(stream.is_finished(), "mock ladder did not finish in time");
    events
}

fn event_kind(event: &BackendEvent) -> &'static str {
    match event {
        BackendEvent::Start { .. } => "start",
        BackendEvent::TurnStart { .. } => "turn_start",
        BackendEvent::TurnComplete { .. } => "turn_complete",
        BackendEvent::BriefStart { .. } => "brief_start",
        BackendEvent::BriefComplete { .. } => "brief_complete",
        BackendEvent::Done { .. } => "done",
        BackendEvent::Error { .. } => "error",
        BackendEvent::TurnFailed { .. } => "turn_failed",
        BackendEvent::Diagnostics { .. } => "diagnostics",
    }
}

#[test]
fn test_council_args_spell_engine_cli() {
    let config = RunConfig {
        question: "Q?".to_string(),
        rounds: 2,
        dry_run: true,
        providers: vec!["mistral".to_string()],
        context_files: vec!["notes.md".to_string()],
        ..RunConfig::default()
    };
    assert_eq!(
        council_args(&config),
        vec![
            "run",
            "council.py",
            "--json-stream",
            "--dry-run",
            "--rounds",
            "2",
            "--providers",
            "mistral",
            "--context",
            "notes.md",
            "--max-attempts",
            "1",
            "--request-id",
            "",
            "--",
            "Q?",
        ]
    );
}

#[test]
fn test_app_lifecycle_without_framework() {
    let mut app = App::new(false, None, true);
    assert_eq!(app.active_tab, Tab::Debate);
    assert!(!app.is_running);
    // Fresh checkouts have no reports; startup must reflect the configured directory.
    let reports_dir = std::env::var_os("COUNCIL_REPORTS_DIR").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reports"),
        std::path::PathBuf::from,
    );
    let expected_paths: Vec<_> = scan_sessions(&reports_dir)
        .into_iter()
        .map(|session| session.path_str)
        .collect();
    let actual_paths: Vec<_> = app
        .sessions
        .iter()
        .map(|session| session.path_str.clone())
        .collect();
    assert_eq!(actual_paths, expected_paths);

    app.submit_composer();
    assert!(
        !app.is_running,
        "empty composer question must not start a run"
    );

    app.composer_state.question = "Should we expand?".to_string();
    app.composer_state.rounds = 2;
    app.submit_composer();
    assert!(app.is_running, "dry-run spawn should succeed offline");
    assert_eq!(app.current_question, "Should we expand?");

    assert!(
        app.backend.is_some(),
        "spawn must hand App a backend handle"
    );
    app.cancel_run();
    assert!(!app.is_running);
    assert!(app.backend.is_none());
}

#[test]
fn test_app_navigation_bounds_and_tab_cycle() {
    let mut app = App::new(false, None, true);
    app.claims = extract_claims(&sample_turns());
    app.active_tab = Tab::Claims;
    app.selected_claim = 0;
    app.navigate_down();
    assert_eq!(app.selected_claim, 1);
    app.navigate_down();
    assert_eq!(app.selected_claim, 2);
    app.navigate_down();
    assert_eq!(
        app.selected_claim, 2,
        "selection must clamp at the ledger end"
    );

    for _ in 0..7 {
        app.next_tab();
    }
    assert_eq!(
        app.active_tab,
        Tab::Sessions,
        "cycling 7 steps from Claims (idx 2) wraps once and lands on Sessions (idx 3)"
    );
}

#[test]
fn test_tab_titles_cover_all_views() {
    let titles: Vec<&str> = Tab::ALL.iter().map(|t| t.title()).collect();
    assert_eq!(
        titles,
        vec![
            "Debate Timeline",
            "Decision Brief",
            "Claim Ledger",
            "Session History",
            "Help & Keys",
            "Diagnostics",
        ]
    );
}

#[test]
fn role_selection_survives_transport_serialization() {
    let mut roles = council_ui::roles::defaults();
    roles.get_mut("brief").unwrap().provider = "OpenRouter".into();
    roles.get_mut("brief").unwrap().model = "any/model".into();
    let config = RunConfig {
        roles,
        max_attempts: 1,
        ..RunConfig::default()
    };
    let wire = serde_json::to_string(&config).unwrap();
    let decoded: RunConfig = serde_json::from_str(&wire).unwrap();
    assert_eq!(decoded.roles["brief"].model, "any/model");
    let args = council_args(&decoded);
    let index = args.iter().position(|a| a == "--roles-json").unwrap();
    assert!(args[index + 1].contains("any/model"));
}

#[test]
fn unchallenged_claim_is_never_presented_as_verified() {
    let claims = extract_claims(&sample_turns()[..1]);
    assert!(claims.iter().all(|c| c.verdict == ClaimVerdict::Open));
    assert_eq!(claims[0].verdict.as_str(), "UNCHALLENGED");
}

#[test]
fn checkpoint_restores_role_config_with_python_turn_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = serde_json::to_value(RunConfig {
        roles: council_ui::roles::defaults(),
        ..RunConfig::default()
    })
    .unwrap();
    config["completed_turns"] = serde_json::json!([{"index":1,"model":"Mistral / small","role":"Opening","text":"C1: saved", "seconds":1.0}]);
    let data = serde_json::json!({"question":"Q", "brief":"", "turns":config["completed_turns"], "run_config":config});
    let path = dir.path().join("partial.json");
    fs::write(&path, data.to_string()).unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert_eq!(detail.run_config.unwrap().roles.len(), 6);
    assert_eq!(detail.turns.len(), 1);
}

#[test]
fn incomplete_claims_do_not_enter_the_ledger() {
    let mut turns = sample_turns();
    turns[0].status = TurnStatus::Incomplete;
    turns[1].status = TurnStatus::Failed;
    turns[2].status = TurnStatus::Failed;
    assert!(extract_claims(&turns).is_empty());
}

#[test]
fn history_keeps_partial_status_and_response_diagnostics() {
    let dir = std::env::temp_dir().join(format!("council-diagnostics-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("run.json");
    let record = serde_json::json!({"question":"Offline fixture", "brief":"", "turns":[],
        "failed_turn":{"index":1,"model":"m","role":"Opening","text":"Partial","seconds":2,
        "status":"incomplete","diagnostics":{"request_id":"request-1","finish_reason":"length"}},
        "diagnostics":{"run_id":"run-1","events":[]}});
    fs::write(&path, record.to_string()).unwrap();
    fs::write(
        dir.join("run.events.jsonl"),
        "{\"event\":\"stage_started\",\"pid\":123}\n{\"event\":\"run_failed\"}\n{partial",
    )
    .unwrap();
    let detail = load_session(path.to_str().unwrap()).unwrap();
    assert_eq!(detail.turns[0].status, TurnStatus::Incomplete);
    assert_eq!(detail.turns[0].text, "Partial");
    assert_eq!(detail.turns[0].diagnostics["request_id"], "request-1");
    assert_eq!(detail.diagnostics["events"].as_array().unwrap().len(), 2);
    fs::remove_dir_all(dir).unwrap();
}
