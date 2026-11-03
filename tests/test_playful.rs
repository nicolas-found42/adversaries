//! Child-readable progress must stay honest when work stops or motion is disabled.
use council_ui::{
    app::App,
    model::{TurnState, TurnStatus},
    ui::playful,
};

#[test]
fn journey_distinguishes_debate_brief_and_pause() {
    let mut app = App::new(false, None, true);
    app.is_running = true;
    assert_eq!(playful::journey_state(&app).0, 1);
    let mut turn = TurnState::new(6, "model".into(), "Brief".into());
    turn.status = TurnStatus::Running;
    app.turns.push(turn);
    assert_eq!(playful::journey_state(&app).0, 2);
    assert!(playful::journey_state(&app).1.contains("Writing"));
    app.is_running = false;
    assert!(playful::journey_state(&app).1.contains("paused"));
    app.brief_text = "## Recommendation\nCheck the options.".into();
    assert!(playful::journey_state(&app).1.contains("Check the facts"));
}

fn scene_shapes(ctx: &egui::Context, app: &App, time: f64) -> String {
    let mut output = ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 500.0),
            )),
            ..Default::default()
        },
        |ui| playful::crew(ui, app, false),
    );
    output.textures_delta.clear();
    format!("{:?}", output.shapes)
}

#[test]
fn reduced_motion_freezes_running_characters_and_speaking_waves() {
    let mut app = App::new(false, None, true);
    app.theme.motion_seconds = 0.0;
    app.is_running = true;
    let mut turn = TurnState::new(1, "model".into(), "Opening".into());
    turn.status = TurnStatus::Running;
    app.turns.push(turn);
    let ctx = egui::Context::default();
    scene_shapes(&ctx, &app, 0.0);
    let first = scene_shapes(&ctx, &app, 1.0);
    assert_eq!(first, scene_shapes(&ctx, &app, 2.0));
    app.theme.motion_seconds = 0.2;
    assert_ne!(scene_shapes(&ctx, &app, 2.2), scene_shapes(&ctx, &app, 2.8));
}

#[test]
fn welcome_and_live_journey_text_fit_a_narrow_screen() {
    let mut app = App::new(false, None, true);
    let ctx = egui::Context::default();
    for live in [false, true] {
        app.is_running = live;
        if live {
            app.current_question = "Garden or robot club?".into();
        }
        for _ in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| council_ui::ui::render(ui, &mut app),
            );
            output.textures_delta.clear();
            for shape in output.shapes {
                if let egui::epaint::Shape::Text(text) = shape.shape {
                    let bounds = text.galley.rect.translate(text.pos.to_vec2());
                    if bounds.intersects(shape.clip_rect) {
                        assert!(bounds.max.x <= 320.5, "Overflow: {}", text.galley.text());
                        assert!(bounds.min.x >= -0.5, "Overflow: {}", text.galley.text());
                    }
                }
            }
        }
    }
}

#[test]
fn compact_arguments_lead_with_reading_instead_of_the_six_stage_picker() {
    let mut app = App::new(false, None, true);
    app.current_question = "Garden or robot club?".into();
    app.active_tab = council_ui::model::Tab::Debate;
    app.brief_text = "## Recommendation\nTry a small experiment.".into();
    for index in 1..=6 {
        let mut turn = TurnState::new(index, "Fixture".into(), "Opening".into());
        turn.status = TurnStatus::Complete;
        turn.text = "An idea we can check together.".into();
        app.turns.push(turn);
    }
    let ctx = egui::Context::default();
    let mut labels = Vec::new();
    for _ in 0..4 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(390.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| council_ui::ui::render(ui, &mut app),
        );
        output.textures_delta.clear();
        labels = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::epaint::Shape::Text(text) = &shape.shape {
                    Some((text.galley.text().to_owned(), text.pos.y))
                } else {
                    None
                }
            })
            .collect();
    }
    let reading = labels
        .iter()
        .find(|(text, _)| text == "Reading: stage 1 · Opening")
        .unwrap_or_else(|| panic!("Missing reading heading: {labels:?}"));
    assert!(
        reading.1 < 500.0,
        "The argument should lead in a narrow window"
    );
    assert!(
        !labels
            .iter()
            .any(|(text, _)| text == "6 · Opening · Complete")
    );
    assert!(
        labels
            .iter()
            .any(|(text, _)| text == "An idea we can check together.")
    );
}

#[test]
fn enlarged_crew_keeps_each_role_in_its_own_readable_cell() {
    let mut app = App::new(false, None, true);
    // At 200% scale, a 320-pixel window has 160 layout points.
    for width in [320.0 / 1.5, 320.0 / 2.0] {
        let ctx = egui::Context::default();
        for _ in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| council_ui::ui::render(ui, &mut app),
            );
            output.textures_delta.clear();
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    let egui::epaint::Shape::Text(text) = &shape.shape else {
                        return None;
                    };
                    ["Idea", "Challenge", "Reply", "Brief"]
                        .contains(&text.galley.text())
                        .then_some(text.galley.rect.translate(text.pos.to_vec2()))
                })
                .collect();
            assert_eq!(labels.len(), 4);
            for (index, label) in labels.iter().enumerate() {
                assert!(
                    label.min.x >= 0.0 && label.max.x <= width,
                    "role clipped at width {width}: {label:?}"
                );
                for other in &labels[index + 1..] {
                    assert!(
                        !label.intersects(*other),
                        "role names overlap at width {width}: {label:?} and {other:?}"
                    );
                }
            }
        }
    }
}
