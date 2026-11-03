use crate::app::App;
use egui::{RichText, Ui};

pub fn render(ui: &mut Ui, app: &App) {
    super::theme::heading(ui, "Technical details");
    ui.add(
        egui::Label::new(
            RichText::new(
                "Available timings, provider-reported usage, and cost details. Missing values mean unavailable, not zero. Desktop records are local; browser records live on the Council server. This page is optional.",
            )
            .color(app.theme.text_muted),
        )
        .wrap(),
    );
    ui.add_space(8.0);
    let stages: Vec<_> = app.turns.iter().map(|t| serde_json::json!({"index": t.index, "model": t.model, "role": t.role, "status": t.status, "seconds": t.seconds, "diagnostics": t.diagnostics})).collect();
    let bundle = serde_json::json!({"run": app.diagnostics, "stages": stages});
    let text = serde_json::to_string_pretty(&bundle).unwrap_or_default();
    if ui
        .add_sized(
            [ui.available_width().min(200.0), 44.0],
            egui::Button::new("Copy run details"),
        )
        .clicked()
    {
        ui.ctx().copy_text(text.clone());
    }
    if app.diagnostics.is_null() {
        ui.label(
            RichText::new(
                "No run-level details are recorded. Stage details may still be available below.",
            )
            .color(app.theme.text_muted),
        );
    }
    ui.add_space(12.0);
    ui.push_id("diagnostics", |ui| {
        for turn in &app.turns {
            stage(ui, turn);
        }
        raw_details(ui, "Everything as text", &text);
    });
}

fn stage(ui: &mut Ui, turn: &crate::model::TurnState) {
    super::theme::disclosure(
        ui,
        format!("{} · {} · {:?}", turn.index, turn.role, turn.status),
    )
    .show(ui, |ui| {
        ui.add(egui::Label::new(&turn.model).wrap());
        measurements(ui, turn);
        raw_details(
            ui,
            "Request details",
            &serde_json::to_string_pretty(&turn.diagnostics).unwrap_or_default(),
        );
    });
}

fn measurements(ui: &mut Ui, turn: &crate::model::TurnState) {
    let details = &turn.diagnostics;
    ui.label(if turn.seconds == 0.0 {
        "Elapsed: Zero or unavailable in this record".into()
    } else {
        format!("Recorded elapsed: {:.1}s", turn.seconds)
    });
    for (label, value) in [
        ("Finish reason", &details["finish_reason"]),
        ("Reasoning mode", &details["policy"]["reasoning_mode"]),
        ("Output allowance", &details["policy"]["output_allowance"]),
        ("Input tokens", &details["usage"]["prompt_tokens"]),
        ("Generated tokens", &details["usage"]["completion_tokens"]),
        (
            "Reasoning tokens",
            &details["usage"]["completion_tokens_details"]["reasoning_tokens"],
        ),
        ("Cost", &details["cost"]["amount"]),
        ("Currency", &details["cost"]["currency"]),
        ("Cost source", &details["cost"]["source"]),
        (
            "Capability source",
            &details["policy"]["capabilities"]["source"],
        ),
    ] {
        ui.add(egui::Label::new(format!("{label}: {}", recorded_value(value))).wrap());
    }
}

fn recorded_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "Not recorded".into(),
        serde_json::Value::String(text) if text.trim().is_empty() => "Not recorded".into(),
        serde_json::Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

fn raw_details(ui: &mut Ui, title: &str, text: &str) {
    super::theme::disclosure(ui, title).show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(text).monospace()).wrap());
    });
}
