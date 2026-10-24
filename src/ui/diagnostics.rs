use crate::app::App;
use egui::{RichText, Ui};

pub fn render(ui: &mut Ui, app: &App) {
    ui.label(RichText::new("Run diagnostics").size(22.0).strong());
    ui.label("Saved locally with this decision. No keys or raw reasoning traces are recorded.");
    let stages: Vec<_> = app.turns.iter().map(|t| serde_json::json!({"index": t.index, "model": t.model, "role": t.role, "status": t.status, "seconds": t.seconds, "diagnostics": t.diagnostics})).collect();
    let bundle = serde_json::json!({"run": app.diagnostics, "stages": stages});
    let text = serde_json::to_string_pretty(&bundle).unwrap_or_default();
    if ui.button("Copy diagnostics").clicked() {
        ui.ctx().copy_text(text.clone());
    }
    if app.diagnostics.is_null() {
        ui.weak("Historical session: request events were not recorded.");
    }
    ui.add_space(12.0);
    egui::ScrollArea::vertical()
        .id_salt("diagnostics")
        .show(ui, |ui| {
            for turn in &app.turns {
                stage(ui, turn);
            }
            ui.collapsing("Event history and export preview", |ui| {
                ui.monospace(&text);
            });
        });
}

fn stage(ui: &mut Ui, turn: &crate::model::TurnState) {
    ui.collapsing(
        format!("{} · {} · {:?}", turn.index, turn.role, turn.status),
        |ui| {
            let details = &turn.diagnostics;
            ui.label(&turn.model);
            ui.label(format!(
                "Finish: {} · Reasoning: {} · Output allowance: {}",
                details["finish_reason"],
                details["policy"]["reasoning_mode"],
                details["policy"]["output_allowance"]
            ));
            ui.label(format!(
                "Elapsed: {:.1}s · Input: {} · Generated: {} · Reasoning: {} tokens",
                turn.seconds,
                details["usage"]["prompt_tokens"],
                details["usage"]["completion_tokens"],
                details["usage"]["completion_tokens_details"]["reasoning_tokens"]
            ));
            ui.label(format!(
                "Cost: {} ({})",
                details["cost"]["amount"], details["cost"]["source"]
            ));
            ui.weak(
                details["policy"]["capabilities"]["source"]
                    .as_str()
                    .unwrap_or("Capability source unknown"),
            );
            ui.collapsing("Request details", |ui| {
                ui.monospace(serde_json::to_string_pretty(details).unwrap_or_default());
            });
        },
    );
}
