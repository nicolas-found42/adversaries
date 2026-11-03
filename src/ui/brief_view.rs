use crate::app::App;
use crate::brief::{qualified_export, sections};
use crate::model::Tab;
use crate::ui::{reader, theme};
use egui::Ui;

pub fn render_brief_view(ui: &mut Ui, app: &mut App) {
    if !app.has_brief() {
        missing_brief(ui, app);
        return;
    }
    let practice = app.demo_mode || app.last_config.as_ref().is_some_and(|c| c.dry_run);
    theme::badge(
        ui,
        if practice {
            "BRIEF READY · PRACTICE EXAMPLE"
        } else {
            "BRIEF READY · AI-GENERATED RECOMMENDATION"
        },
        app.theme.success,
    );
    ui.label("AI wrote this guide. Check the facts before choosing. Model-generated content, not independently verified. Completion does not establish consensus.");
    recommendation_and_sections(ui, app, practice);
    crate::ui::theme::disclosure(ui, "Original generated brief").show(ui, |ui| {
        reader::body(ui, &app.brief_text, &mut app.markdown_cache);
    });
    crate::ui::theme::disclosure(ui, "Trace a claim to its argument").show(ui, |ui| {
        let mut open = None;
        for (index, claim) in app.claims.iter().enumerate() {
            let scope = egui::Id::unique((
                "brief_claim_trace",
                app.workspace_generation,
                claim.origin_key(),
            ));
            let inspect = ui
                .scope_builder(egui::UiBuilder::new().scope_id(scope), |ui| {
                    ui.button(format!(
                        "{} · stage {} · {}",
                        claim.id, claim.turn_index, claim.author_model
                    ))
                })
                .inner;
            super::restore_source_focus(ui, &mut app.pending_focus, &inspect);
            if inspect.clicked() {
                open = Some((index, inspect.id));
            }
        }
        if let Some((index, focus)) = open {
            app.inspect_claim(index);
            app.source_return_focus = Some(focus);
        }
    });
}

fn missing_brief(ui: &mut Ui, app: &mut App) {
    reader::placeholder(
        ui,
        "No decision brief is available",
        "Read completed arguments while work continues. An interrupted or unfinished debate is not a recommendation.",
    );
    let available = app
        .turns
        .iter()
        .any(|turn| turn.status == crate::model::TurnStatus::Complete);
    if ui
        .add_enabled(available, egui::Button::new("Read completed arguments"))
        .clicked()
    {
        app.read_completed_arguments();
    }
    if !available {
        ui.weak("Completed arguments appear after a stage finishes.");
    }
}

fn brief_actions(ui: &mut Ui, app: &mut App, practice: bool) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Copy qualified brief").clicked() {
            ui.ctx()
                .copy_text(qualified_export(&app.brief_text, practice));
        }
        if ui.button("Inspect claims & sources").clicked() {
            app.active_tab = Tab::Claims;
        }
    });
}

fn recommendation_and_sections(ui: &mut Ui, app: &mut App, practice: bool) {
    let parsed = sections(&app.brief_text);
    let Some(recommendation) = crate::brief::recommendation(&app.brief_text) else {
        brief_actions(ui, app, practice);
        reader::document(
            ui,
            ("brief", &app.current_question),
            &app.brief_text,
            &mut app.markdown_cache,
        );
        return;
    };
    theme::feature_card(&app.theme, app.theme.success).show(ui, |ui| {
        crate::ui::theme::heading(ui, "Recommendation");
        reader::body(ui, &recommendation, &mut app.markdown_cache);
    });
    brief_actions(ui, app, practice);
    let inline = crate::brief::inline_value(&app.brief_text, "Recommendation:").is_some();
    if let Some(confidence) = crate::brief::inline_value(&app.brief_text, "Confidence:") {
        crate::ui::theme::heading(ui, "Model-reported confidence");
        ui.label("Model-reported estimate, not a calibrated probability.");
        reader::body(ui, &confidence, &mut app.markdown_cache);
    }
    for (index, section) in parsed
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.title.eq_ignore_ascii_case("recommendation") && !s.body.is_empty())
    {
        if inline && index == 0 {
            crate::ui::theme::disclosure(ui, "Original opening text").show(ui, |ui| {
                reader::body(ui, &section.body, &mut app.markdown_cache);
            });
        } else {
            brief_section(ui, app, section);
        }
    }
}

fn brief_section(ui: &mut Ui, app: &mut App, section: &crate::brief::BriefSection) {
    crate::ui::theme::heading(
        ui,
        if section.title.eq_ignore_ascii_case("confidence") {
            "Model-reported confidence"
        } else {
            &section.title
        },
    );
    if section.title.eq_ignore_ascii_case("confidence") {
        ui.label("Model-reported estimate, not a calibrated probability.");
    }
    reader::document(
        ui,
        ("section", &app.current_question, &section.title),
        &section.body,
        &mut app.markdown_cache,
    );
}
