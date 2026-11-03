use super::{App, BackendHandle, ExecutionReview, ReviewSource, RunConfig, Tab};
use crate::model::{TurnState, TurnStatus};

fn previous_work() -> App {
    let mut app = App::new(false, None, true);
    app.current_question = "Original decision — unsaved café".into();
    app.brief_text = "## Recommendation\n\nKeep the original brief.".into();
    let mut turn = TurnState::new(1, "Original / model".into(), "Opening".into());
    turn.text = "C1: Original retained statement.".into();
    turn.status = TurnStatus::Complete;
    app.turns.push(turn);
    app.claims = crate::claims::extract_claims(&app.turns);
    app.active_tab = Tab::Brief;
    app.session_key = "original-session".into();
    app.persisted_stages = 0;
    app.last_config = Some(RunConfig {
        question: app.current_question.clone(),
        rounds: 1,
        ..Default::default()
    });
    app
}

fn reviewed() -> ExecutionReview {
    ExecutionReview {
        source: ReviewSource::Repeat,
        config: RunConfig {
            question: "Replacement decision".into(),
            rounds: 1,
            ..Default::default()
        },
        message: String::new(),
        preflight: None,
    }
}

#[test]
fn failed_launch_preserves_the_previous_unsaved_reader_and_configuration() {
    let mut app = previous_work();
    let generation = app.workspace_generation;
    app.execute_review_using(reviewed(), |_| Err("Synthetic launch failure".into()));
    assert_eq!(app.current_question, "Original decision — unsaved café");
    assert_eq!(
        app.brief_text,
        "## Recommendation\n\nKeep the original brief."
    );
    assert_eq!(app.turns[0].text, "C1: Original retained statement.");
    assert_eq!(app.claims.len(), 1);
    assert_eq!(
        app.last_config.as_ref().unwrap().question,
        app.current_question
    );
    assert_eq!(app.session_key, "original-session");
    assert_eq!(app.workspace_generation, generation);
    assert_eq!(app.active_tab, Tab::Brief);
    assert_eq!(app.persisted_stages, 0);
    assert!(!app.is_running);
    assert!(
        app.launch_failure
            .as_ref()
            .unwrap()
            .message
            .contains("Synthetic launch failure")
    );
    assert_eq!(
        app.launch_failure.as_ref().unwrap().question,
        "Replacement decision"
    );
    assert!(app.error.is_none());
    assert!(app.backend.is_none());
}

#[test]
fn a_successful_launch_replaces_the_workspace_only_after_a_handle_is_available() {
    let mut app = previous_work();
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = BackendHandle::new(receiver, std::sync::Arc::new(|| {}));
    let mut calls = 0;
    app.execute_review_using(reviewed(), |config| {
        calls += 1;
        assert_eq!(config.question, "Replacement decision");
        Ok(handle)
    });
    assert_eq!(calls, 1);
    assert_eq!(app.current_question, "Replacement decision");
    assert!(app.brief_text.is_empty());
    assert!(app.turns.is_empty());
    assert!(app.is_running && app.backend.is_some());
    assert!(app.error.is_none());
    assert_eq!(app.active_tab, Tab::Debate);
    drop(sender);
}

#[test]
fn failed_replacement_stops_the_owned_run_but_keeps_its_output_and_failure_context() {
    let mut app = previous_work();
    app.error = Some("Previous save failure".into());
    let (sender, receiver) = std::sync::mpsc::channel();
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = cancelled.clone();
    app.backend = Some(BackendHandle::new(
        receiver,
        std::sync::Arc::new(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }),
    ));
    app.is_running = true;
    app.execute_review_using(reviewed(), |_| Err("Replacement unavailable".into()));
    assert_eq!(cancelled.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!app.is_running && app.stopped_by_user);
    assert_eq!(
        app.brief_text,
        "## Recommendation\n\nKeep the original brief."
    );
    assert_eq!(app.error.as_deref(), Some("Previous save failure"));
    assert!(app.backend.is_none());
    drop(sender);
}
