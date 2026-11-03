//! Browser semantics are derived from the actual shared UI, not hand-copied labels.
use council_ui::{accessibility, app::App};
use egui::accesskit::{Action, ActionData, ActionRequest, TreeId};
use serde_json::Value;

fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> Value {
    render_frame(ctx, app, events).0
}

fn render_frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> (Value, egui::Id) {
    let mut workspace = egui::Id::NULL;
    let mut tree = Value::Null;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(620.0, 400.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            workspace = council_ui::ui::render_with_workspace(ui, app);
            tree = serde_json::to_value(accessibility::snapshot(ctx)).unwrap();
        },
    );
    output.textures_delta.clear();
    (tree, workspace)
}

fn find<'a>(tree: &'a Value, predicate: &impl Fn(&Value) -> bool) -> Option<&'a Value> {
    if predicate(tree) {
        return Some(tree);
    }
    tree["children"]
        .as_array()?
        .iter()
        .find_map(|node| find(node, predicate))
}

fn action(node: &Value, action: Action, data: Option<ActionData>) -> egui::Event {
    egui::Event::AccessKitActionRequest(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: node["id"].as_str().unwrap().parse::<u64>().unwrap().into(),
        data,
    })
}

#[test]
fn semantics_open_composer_and_edit_the_same_question_without_dispatch() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    let tree = frame(&ctx, &mut app, Vec::new());
    let button = find(&tree, &|node| node["label"] == "New decision").unwrap();
    frame(&ctx, &mut app, vec![action(button, Action::Click, None)]);
    assert!(app.composer_open);
    let tree = frame(&ctx, &mut app, Vec::new());
    let input = find(&tree, &|node| {
        node["role"] == "MultilineTextInput" && node["set_value"] == true
    })
    .unwrap();
    frame(
        &ctx,
        &mut app,
        vec![action(
            input,
            Action::SetValue,
            Some(ActionData::Value("Which option should we choose?".into())),
        )],
    );
    assert_eq!(
        app.composer_state.question,
        "Which option should we choose?"
    );
    assert!(app.backend.is_none());
}

#[test]
fn semantic_reader_includes_content_beyond_the_canvas_viewport() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    app.current_question = "A long saved decision".into();
    app.active_tab = council_ui::model::Tab::Brief;
    app.brief_text = format!(
        "# Recommendation\n\n{}\n\nUnique end of the original brief.",
        "A paragraph of rationale.\n\n".repeat(80)
    );
    let tree = frame(&ctx, &mut app, Vec::new());
    let serialized = serde_json::to_string(&tree).unwrap();
    assert!(serialized.contains("Unique end of the original brief."));
    assert!(!serialized.contains("TextRun"));
}

#[test]
fn semantic_approval_dispatches_a_ready_practice_plan_once() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(true, None, true);
    app.start_run("Offline semantic approval fixture".into(), 1, true);
    app.execution_review
        .as_mut()
        .unwrap()
        .preflight
        .as_mut()
        .unwrap()
        .providers = ["Mistral", "OpenRouter", "Ollama-Cloud", "OpenCode-Go"]
        .iter()
        .map(|name| council_ui::preflight::ProviderCheck {
            name: (*name).into(),
            configured: true,
        })
        .collect();
    assert!(app.execution_review.as_ref().unwrap().ready());
    let mut tree = frame(&ctx, &mut app, Vec::new());
    for _ in 0..3 {
        tree = frame(&ctx, &mut app, Vec::new());
    }
    let button = find(&tree, &|node| node["label"] == "Run practice example").unwrap();
    assert_eq!(button["disabled"], false);
    frame(
        &ctx,
        &mut app,
        vec![
            action(button, Action::Focus, None),
            action(button, Action::Click, None),
        ],
    );
    assert!(app.execution_review.is_none());
    assert!(app.backend.is_some());
    app.cancel_run();
}

#[test]
fn source_navigation_tags_the_drawn_content_before_the_next_view_appears() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    app.current_question = "Source navigation fixture".into();
    let mut turn = council_ui::model::TurnState::new(1, "Mistral".into(), "Opening".into());
    turn.text = "C1: An original statement.".into();
    turn.status = council_ui::model::TurnStatus::Complete;
    app.turns = vec![turn];
    app.claims = council_ui::claims::extract_claims(&app.turns);
    app.active_tab = council_ui::model::Tab::Claims;
    let tree = frame(&ctx, &mut app, Vec::new());
    let before = council_ui::ui::workspace_key(&app);
    let inspect = find(&tree, &|node| {
        node["label"] == "Inspect statement, challenges and response"
    })
    .unwrap();
    let (_, drawn) = render_frame(&ctx, &mut app, vec![action(inspect, Action::Click, None)]);
    assert_eq!(drawn, before, "the click frame still draws the claim list");
    assert_ne!(drawn, council_ui::ui::workspace_key(&app));
    let (source, drawn) = render_frame(&ctx, &mut app, Vec::new());
    assert_eq!(drawn, council_ui::ui::workspace_key(&app));
    let back = find(&source, &|node| {
        node["label"] == "< Back to Claims & sources"
    })
    .unwrap();
    assert_eq!(
        ctx.memory(egui::Memory::focused)
            .unwrap()
            .value()
            .to_string(),
        back["id"].as_str().unwrap()
    );
}

#[test]
fn text_size_choice_updates_scale_and_closes_its_popup() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    let tree = frame(&ctx, &mut app, Vec::new());
    let menu = find(&tree, &|node| {
        node["role"] == "ComboBox" && node["value"] == "Text size"
    })
    .unwrap();
    frame(&ctx, &mut app, vec![action(menu, Action::Click, None)]);
    let tree = frame(&ctx, &mut app, Vec::new());
    let choice = find(&tree, &|node| node["label"] == "Extra large").unwrap();
    frame(&ctx, &mut app, vec![action(choice, Action::Click, None)]);
    let tree = frame(&ctx, &mut app, Vec::new());
    assert!((ctx.zoom_factor() - 1.5).abs() < f32::EPSILON);
    assert!(find(&tree, &|node| node["label"] == "Extra large").is_none());
}

#[test]
fn source_comparison_exposes_both_texts_and_the_relationship_limit() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    app.current_question = "Comparison fixture".into();
    for (index, text) in [
        (1, "C1: Original discount is 60%."),
        (2, "C1: Revised discount is 50%."),
    ] {
        let mut turn =
            council_ui::model::TurnState::new(index, "Fixture model".into(), "Opening".into());
        turn.text = text.into();
        turn.status = council_ui::model::TurnStatus::Complete;
        app.turns.push(turn);
    }
    app.claims = council_ui::claims::extract_claims(&app.turns);
    app.inspect_claim(0);
    let tree = frame(&ctx, &mut app, Vec::new());
    let text = serde_json::to_string(&tree).unwrap();
    for expected in [
        "Original discount is 60%.",
        "Revised discount is 50%.",
        "Same label does not establish a revision, agreement or support.",
    ] {
        assert!(text.contains(expected), "Missing {expected}");
    }
}

#[test]
fn unfinished_brief_exposes_one_notice_and_qualified_recovery_actions() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    app.current_question = "An unfinished decision".into();
    app.active_tab = council_ui::model::Tab::Brief;
    let mut turn = council_ui::model::TurnState::new(1, "Model".into(), "Opening".into());
    turn.status = council_ui::model::TurnStatus::Complete;
    turn.text = "An available completed argument.".into();
    app.turns = vec![turn];
    for _ in 0..3 {
        let tree = frame(&ctx, &mut app, Vec::new());
        let text = serde_json::to_string(&tree).unwrap();
        assert!(text.contains("Unfinished — no decision brief"));
        assert!(!text.contains("No decision brief is available"));
        assert!(find(&tree, &|node| node["label"] == "Copy completed arguments").is_some());
    }
}

#[test]
fn help_exposes_task_sections_as_headings_and_qualifies_the_result() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    app.active_tab = council_ui::model::Tab::Help;
    let tree = frame(&ctx, &mut app, Vec::new());
    for title in [
        "Reading the result",
        "Before a real run",
        "If work stops or cannot be opened",
        "Keyboard",
        "Where things are kept",
    ] {
        assert!(
            find(&tree, &|node| node["role"] == "Heading"
                && node["label"] == title)
            .is_some(),
            "missing heading: {title}"
        );
    }
    let content = tree.to_string();
    assert!(content.contains("not independent verification"));
    assert!(content.contains("not a calibrated probability"));
    assert!(content.contains("Review remaining work"));
    assert!(content.contains("Opening saved work does not run models"));
    assert!(app.backend.is_none());
}

#[test]
fn markdown_projection_groups_inline_content_and_keeps_product_actions() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    app.current_question = "Reader flow regression fixture".into();
    app.active_tab = council_ui::model::Tab::Debate;
    let original = "C1: [SOURCE EXCERPT MATCHED] A **bold** claim with `inline code`.\n\nA separate paragraph.";
    let mut turn = council_ui::model::TurnState::new(1, "Fixture model".into(), "Opening".into());
    turn.text = original.into();
    turn.status = council_ui::model::TurnStatus::Complete;
    app.turns = vec![turn];
    let tree = frame(&ctx, &mut app, Vec::new());
    let document =
        find(&tree, &|node| node["document"].is_object()).expect("shared reader document");
    assert_eq!(document["children"].as_array().unwrap().len(), 0);
    let blocks = document["document"]["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(
        blocks[0]["children"][0]["text"],
        "C1: [SOURCE EXCERPT MATCHED] A "
    );
    assert_eq!(blocks[0]["children"][1]["kind"], "strong");
    assert_eq!(app.turns[0].text, original, "original output is untouched");
    assert!(
        find(&tree, &|node| node["label"] == "Decision actions"
            && node["click"] == true)
        .is_some()
    );
    app.active_tab = council_ui::model::Tab::Help;
    let tree = frame(&ctx, &mut app, Vec::new());
    assert!(
        find(&tree, &|node| node["document"].is_object()).is_none(),
        "the prior frame's document must not leak into another scope"
    );
}

#[test]
fn shared_landmarks_follow_reading_order_and_contain_their_controls() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    app.current_question = "A decision with clear reading landmarks".into();
    app.brief_text = "# Recommendation\n\nRead the supplied evidence before deciding.".into();
    let tree = frame(&ctx, &mut app, Vec::new());
    let panels = find(&tree, &|node| {
        node["children"]
            .as_array()
            .is_some_and(|children| children.iter().any(|child| child["role"] == "Pane"))
    })
    .unwrap();
    let children = panels["children"].as_array().unwrap();
    assert!(find(&children[0], &|node| node["role"] == "Navigation").is_some());
    assert!(find(&children[1], &|node| node["role"] == "Main").is_some());
    assert!(find(&children[2], &|node| node["role"] == "ContentInfo").is_some());
    let navigation = find(&tree, &|node| node["role"] == "Navigation").unwrap();
    assert_eq!(navigation["label"], "Council navigation");
    assert!(
        find(navigation, &|node| node["label"] == "New decision"
            && node["click"] == true)
        .is_some()
    );
    let main = find(&tree, &|node| node["role"] == "Main").unwrap();
    assert!(
        find(main, &|node| node["role"] == "Heading"
            && node["level"] == 1
            && node["label"] == app.current_question)
        .is_some()
    );
    let views = find(main, &|node| {
        node["role"] == "Group" && node["label"] == "Decision views"
    })
    .unwrap();
    assert_eq!(views["horizontal"], true);
    assert!(
        find(views, &|node| node["label"] == "Claims & sources"
            && node["click"] == true)
        .is_some()
    );
    let footer = find(&tree, &|node| node["role"] == "ContentInfo").unwrap();
    assert!(
        find(footer, &|node| node["role"] == "Status"
            && node["live"] == true)
        .is_some()
    );
    assert!(app.backend.is_none());
}

#[test]
fn history_and_help_keep_primary_titles_in_the_shared_heading_hierarchy() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(false, None, true);
    for (tab, title) in [
        (council_ui::model::Tab::Sessions, "All decisions"),
        (council_ui::model::Tab::Help, "How this works"),
    ] {
        app.active_tab = tab;
        let tree = frame(&ctx, &mut app, Vec::new());
        let main = find(&tree, &|node| node["role"] == "Main").unwrap();
        assert!(
            find(main, &|node| node["role"] == "Heading"
                && node["level"] == 1
                && node["label"] == title)
            .is_some()
        );
    }
    assert!(app.backend.is_none());
}
