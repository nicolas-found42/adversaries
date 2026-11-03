//! User-visible contracts for execution review, provenance, and reading.
mod common;
use council_ui::app::App;
use council_ui::claims::extract_claims;
use council_ui::execution::ReviewSource;
use council_ui::model::{Tab, TurnState, TurnStatus};
use council_ui::transport::RunConfig;

fn completed(index: usize, text: &str) -> TurnState {
    let mut turn = TurnState::new(index, "Mistral / small".into(), "Opening".into());
    turn.text = text.into();
    turn.status = TurnStatus::Complete;
    turn
}

#[test]
fn editing_and_canceling_review_never_start_a_paid_run() {
    let mut app = App::new(false, None, false);
    app.start_run("A real question".into(), 1, false);
    assert!(app.execution_review.is_some());
    assert!(app.backend.is_none());
    assert!(!app.is_running);
    app.execution_review = None;
    app.confirm_execution();
    assert!(app.backend.is_none());
}

#[test]
fn changed_inputs_invalidate_review_before_dispatch() {
    let mut app = App::new(false, None, false);
    app.start_run("A real question".into(), 1, false);
    app.composer_state.context_files.push("changed.pdf".into());
    app.confirm_execution();
    let review = app.execution_review.as_ref().unwrap();
    assert!(review.message.contains("changed"));
    assert_eq!(review.config.context_files, ["changed.pdf"]);
    assert!(app.backend.is_none());
}

#[test]
fn continuation_retains_only_complete_work_and_repeat_retains_none() {
    let mut app = App::new(false, None, false);
    app.last_config = Some(RunConfig {
        question: "Q".into(),
        rounds: 2,
        ..RunConfig::default()
    });
    app.turns.push(completed(1, "C1: original"));
    app.turns
        .push(TurnState::new(2, "m".into(), "Red team 1".into()));
    app.retry_stage();
    let review = app.execution_review.as_ref().unwrap();
    assert_eq!(review.retained(), 1);
    assert_eq!(review.remaining(), 9);
    assert!(app.backend.is_none());
    app.rerun_current();
    assert_eq!(app.execution_review.as_ref().unwrap().retained(), 0);
    assert_eq!(
        app.turns.len(),
        2,
        "review must not clear the readable record"
    );
}

#[test]
fn live_completion_invalidates_an_open_continuation_plan() {
    let mut app = App::new(false, None, false);
    app.last_config = Some(RunConfig {
        question: "Q".into(),
        rounds: 1,
        ..RunConfig::default()
    });
    app.retry_stage();
    app.turns.push(completed(1, "C1: arrived while reviewing"));
    app.confirm_execution();
    assert!(app.execution_review.is_some());
    assert!(app.backend.is_none());
    assert_eq!(
        app.review_config(ReviewSource::Continue)
            .unwrap()
            .completed_turns
            .len(),
        1
    );
}

#[test]
fn source_spans_keep_full_unicode_and_distinct_versions() {
    let text = format!("Intro.\nC1: Nairobi — {}\n", "important detail ".repeat(30));
    let turns = vec![completed(1, &text), completed(2, "C1: A later statement.")];
    let claims = extract_claims(&turns);
    assert_eq!(claims.len(), 2);
    assert!(claims[0].text.len() > 120);
    assert_ne!(claims[0].origin_key(), claims[1].origin_key());
    assert_eq!(
        &text[claims[0].source_start..claims[0].source_end],
        text.lines().nth(1).unwrap()
    );
}

#[test]
fn inspecting_a_claim_disables_follow_and_retains_selection() {
    let mut app = App::new(false, None, true);
    app.turns = vec![completed(1, "C1: original")];
    app.claims = extract_claims(&app.turns);
    app.inspect_claim(0);
    assert_eq!(app.active_tab, Tab::Debate);
    assert_eq!(app.source_claim, Some(0));
    assert!(!app.follow_live);
    assert_eq!(app.claims[0].id, "C1");
}

#[test]
fn exported_brief_does_not_drop_uncertainty_qualification() {
    let text = council_ui::brief::qualified_export("72% — moderate-high.", true);
    assert!(text.contains("simulated"));
    assert!(text.contains("not a calibrated probability"));
    assert!(text.contains("72% — moderate-high."));
}

fn rendered_texts(app: &mut App, width: f32) -> Vec<(String, egui::Rect)> {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 900.0),
        )),
        ..Default::default()
    };
    let mut texts = Vec::new();
    for _ in 0..5 {
        let mut output = ctx.run_ui(input.clone(), |ui| council_ui::ui::render(ui, app));
        texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => {
                    let bounds = text.galley.rect.translate(text.pos.to_vec2());
                    bounds
                        .intersects(shape.clip_rect)
                        .then(|| (text.galley.text().to_owned(), bounds))
                }
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
    }
    texts
}

#[test]
fn narrow_readers_wrap_instead_of_clipping_horizontally() {
    let mut app = App::new(false, None, true);
    app.current_question = "Should we expand our engineering team in Nairobi or London?".into();
    app.turns = vec![completed(
        1,
        &format!("C1: {}", "Important decision evidence. ".repeat(12)),
    )];
    app.claims = extract_claims(&app.turns);
    app.brief_text = format!(
        "# Brief\n\n## Recommendation\n\n{}\n",
        "Consider this tradeoff carefully. ".repeat(12)
    );
    for tab in [Tab::Brief, Tab::Debate, Tab::Claims] {
        app.active_tab = tab;
        for width in [320.0, 620.0] {
            for (text, rect) in rendered_texts(&mut app, width) {
                assert!(
                    rect.max.x <= width + 0.5,
                    "{tab:?} at {width}: text overflowed: {rect:?}: {text}"
                );
            }
        }
    }
}

#[test]
fn approval_is_consumed_and_cannot_be_replayed() {
    let mut app = App::new(false, None, true);
    app.demo_mode = true;
    app.start_run("Q".into(), 1, true);
    app.confirm_execution();
    assert!(app.execution_review.is_none());
    assert!(app.backend.is_some());
    app.cancel_run();
    app.confirm_execution();
    assert!(
        app.backend.is_none(),
        "a consumed approval cannot dispatch again"
    );
}

#[test]
fn claim_keyboard_navigation_respects_the_visible_filter() {
    let mut app = App::new(false, None, true);
    app.turns = vec![completed(
        1,
        "C1: hidden\nC2: relevant\nC3: hidden\nC4: relevant",
    )];
    app.claims = extract_claims(&app.turns);
    app.claim_search = "relevant".into();
    app.active_tab = Tab::Claims;
    app.navigate_down();
    assert_eq!(app.selected_claim, 1);
    app.navigate_down();
    assert_eq!(app.selected_claim, 3);
    app.navigate_up();
    assert_eq!(app.selected_claim, 1);
}

#[test]
fn reused_claim_ids_do_not_receive_an_ambiguous_rebuttal_verdict() {
    let mut response = completed(3, "Defending C1: the claim stands.");
    response.role = "Rebuttal".into();
    let claims = extract_claims(&[
        completed(1, "C1: first"),
        completed(2, "C1: second"),
        response,
    ]);
    assert_eq!(claims.len(), 2);
    for claim in claims {
        assert_eq!(claim.verdict, council_ui::model::ClaimVerdict::Open);
        assert_eq!(claim.ambiguous_references, [3]);
    }
}

#[test]
fn supplied_excerpt_inspection_uses_the_frozen_input_text() {
    let claims = extract_claims(&[completed(
        1,
        "C1: [SOURCE EXCERPT MATCHED] Budget fixed | source: \"€50 budget\"",
    )]);
    let config = RunConfig {
        context_snapshot: Some(vec![council_ui::preflight::DocumentSnapshot {
            name: "notes.md".into(),
            text: "Original: €50\n budget allocated.".into(),
            ..Default::default()
        }]),
        ..RunConfig::default()
    };
    let sources = council_ui::provenance::supplied_excerpts(&claims[0], "Q", Some(&config));
    assert_eq!(sources[0].name, "notes.md");
    assert_eq!(sources[0].excerpt, "€50\n budget");
    assert!(council_ui::provenance::supplied_excerpts(&claims[0], "Q", None).is_empty());
}

#[test]
fn history_distinguishes_missing_directory_from_unreadable_storage() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("no-history-yet");
    assert!(
        council_ui::session_loader::scan_sessions_checked(&missing)
            .unwrap()
            .is_empty()
    );
    let invalid = directory.path().join("not-a-directory");
    std::fs::write(&invalid, "Fixture").unwrap();
    assert!(council_ui::session_loader::scan_sessions_checked(&invalid).is_err());
}

#[test]
fn failed_history_open_keeps_the_current_decision_and_its_settings() {
    let mut app = App::new(false, None, true);
    app.current_question = "Existing decision".into();
    app.brief_text = "Existing readable brief".into();
    app.last_config = Some(RunConfig {
        question: app.current_question.clone(),
        ..Default::default()
    });
    app.load_session_by_path("/nonexistent/council-fixture.json");
    common::finish_storage(&mut app);
    assert!(app.load_failed());
    assert_eq!(app.current_question, "Existing decision");
    assert_eq!(app.brief_text, "Existing readable brief");
    assert_eq!(app.last_config.unwrap().question, "Existing decision");
}

#[test]
fn each_approval_has_an_independent_browser_safe_request_identity() {
    let first = council_ui::transport::new_request_id();
    let second = council_ui::transport::new_request_id();
    assert_ne!(first, second);
    let parsed = uuid::Uuid::parse_str(&first).unwrap();
    assert_eq!(parsed.get_version_num(), 4);
}

#[test]
fn a_new_approved_run_does_not_inherit_the_previous_reader_position_or_follow_setting() {
    let mut app = App::new(true, None, true);
    app.start_run("Repeated decision".into(), 1, true);
    app.confirm_execution();
    let first_reader = council_ui::ui::workspace_key(&app);
    app.cancel_run();
    app.follow_live = false;
    app.start_run("Repeated decision".into(), 1, true);
    app.confirm_execution();
    assert!(
        app.follow_live,
        "a newly approved run starts by following progress"
    );
    assert_ne!(first_reader, council_ui::ui::workspace_key(&app));
    app.cancel_run();
}

#[test]
fn comparison_keeps_verbatim_uses_and_excludes_partial_output_and_other_labels() {
    let mut turns = vec![
        completed(1, "C1: Original value is 60%.\nC10: A different statement."),
        completed(2, "C1: Revised value is 50%."),
        completed(3, "C1: Partial value must not enter the comparison."),
    ];
    turns[2].status = TurnStatus::Incomplete;
    let claims = extract_claims(&turns);
    let uses = council_ui::claims::references(&turns, &claims[0]);
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].text, "C1: Revised value is 50%.");
    assert_eq!(uses[0].turn.index, 2);
    let earlier = council_ui::claims::references(&turns, &claims[2]);
    assert_eq!(earlier[0].text, "C1: Original value is 60%.");
}

#[test]
fn recovery_export_preserves_complete_text_and_excludes_partial_output() {
    let mut incomplete = completed(2, "Partial output must stay out.");
    incomplete.status = TurnStatus::Failed;
    let turns = vec![completed(1, "C1: Verbatim ünicode claim."), incomplete];
    for practice in [true, false] {
        let copy = council_ui::brief::completed_arguments_export("A or B?", &turns, practice);
        assert!(copy.contains("Incomplete debate — no decision brief"));
        assert!(copy.contains("Question: A or B?"));
        assert!(copy.contains("C1: Verbatim ünicode claim."));
        assert!(copy.contains("does not confirm saved storage"));
        assert!(copy.contains("Not independently verified"));
        assert!(!copy.contains("Partial output must stay out."));
        assert_eq!(
            copy.contains("Practice example: simulated content."),
            practice
        );
    }
}
