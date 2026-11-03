//! Consequential actions share one review boundary on desktop and WASM.
use crate::app::App;
use crate::execution::{ExecutionReview, ReviewSource};
use egui::{Context, RichText, Ui};

pub fn show(ctx: &Context, app: &mut App) {
    let Some(review) = app.execution_review.clone() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("execution_review")).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 40.0).clamp(80.0, 680.0));
        let compact = ui.available_width() < 500.0 || ctx.content_rect().height() < 650.0;
        if compact {
            egui::ScrollArea::vertical()
                .id_salt("compact_execution_review")
                .max_height((ctx.content_rect().height() - 40.0).max(80.0))
                .show(ui, |ui| {
                    crate::ui::theme::heading(ui, "Review before starting");
                    plan_body(ui, app, &review);
                    ui.separator();
                    actions(ui, app, &review);
                });
        } else {
            crate::ui::theme::heading(ui, "Review before starting");
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 220.0).max(120.0))
                .show(ui, |ui| plan_body(ui, app, &review));
            ui.separator();
            actions(ui, app, &review);
        }
    });
    if response.should_close() {
        app.execution_review = None;
    }
}

fn plan_body(ui: &mut Ui, app: &App, review: &ExecutionReview) {
    ui.label(RichText::new(&review.config.question).strong());
    ui.label(if review.config.dry_run {
        "Practice example · No inference charges"
    } else {
        "Real provider run · Charges may apply"
    });
    ui.label(format!(
        "{} debate round(s) · {} planned stages · {} retained · {} to execute",
        review.config.rounds,
        review.total(),
        review.retained(),
        review.remaining()
    ));
    ui.label(if review.source == ReviewSource::Continue {
        "Keep completed arguments. Retry the unfinished stage, then execute the remaining stages with the saved assignments."
    } else { "Start a new run. Existing arguments are not reused. Previously saved decisions remain in history." });
    if review.source != ReviewSource::Continue && app.has_workspace() {
        ui.label("Starting replaces the current decision in this window. Only confirmed saved output can be reopened from history. Cancel and copy any unsaved work before starting.");
    }
    if app.is_running {
        ui.colored_label(
            app.theme.warning,
            "Starting this plan stops the active run. In-flight requests may still be charged.",
        );
    }
    recipients(ui, review);
    input_check(ui, review);
    ui.label(if review.config.dry_run {
        "The engine uses simulated responses. No inference request is sent to a model provider. Browser attachments may already have been uploaded to the Council server."
    } else {
        "Your question and included document content will be sent to the listed providers. Provider charges may apply. Council cannot confirm the total cost before this run. Stopping does not guarantee a refund."
    });
    ui.label(RichText::new("Completion means a model-generated brief is available. It does not establish consensus or independent verification.").small());
    if !review.message.is_empty() {
        ui.colored_label(app.theme.warning, &review.message);
    }
}

fn recipients(ui: &mut Ui, review: &ExecutionReview) {
    ui.separator();
    ui.strong("Recipients and role assignments");
    for (key, title) in crate::roles::ROLES {
        if let Some(selection) = review.config.roles.get(key) {
            ui.label(format!(
                "{title}: {} / {}",
                selection.provider, selection.model
            ));
        }
    }
    for provider in &review.config.providers {
        ui.label(format!("Provider: {provider}"));
    }
    ui.separator();
    ui.strong("Included documents");
    if review.config.context_files.is_empty() {
        ui.label("No documents included.");
    }
    for (index, file) in review.config.context_files.iter().enumerate() {
        ui.label(format!(
            "Document {} · {}",
            index + 1,
            crate::attachments::display_name(file)
        ));
    }
    if !review.config.context_files.is_empty() {
        crate::ui::theme::disclosure(ui, "Document storage references").show(ui, |ui| {
            for (index, file) in review.config.context_files.iter().enumerate() {
                ui.label(format!("Document {}: {file}", index + 1));
            }
        });
    }
}

fn actions(ui: &mut Ui, app: &mut App, review: &ExecutionReview) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                review.ready(),
                egui::Button::new(RichText::new(review.action_label()).strong()),
            )
            .clicked()
        {
            app.confirm_execution();
        }
        if ui
            .button(if review.source == ReviewSource::Continue {
                "Configure a new run"
            } else {
                "Back to edit"
            })
            .clicked()
        {
            app.edit_execution();
        }
        if ui
            .add_enabled(
                review.preflight.is_some(),
                egui::Button::new("Recheck inputs"),
            )
            .clicked()
        {
            app.retry_preflight();
        }
        if ui.button("Cancel").clicked() {
            app.execution_review = None;
        }
    });
}

fn input_check(ui: &mut Ui, review: &ExecutionReview) {
    ui.separator();
    ui.strong("Input check — no inference calls");
    let Some(report) = &review.preflight else {
        ui.spinner();
        ui.label("Extracting document text and checking provider credentials…");
        return;
    };
    if !report.error.is_empty() {
        ui.label(&report.error);
    }
    for provider in &report.providers {
        ui.label(format!(
            "{}: {}",
            provider.name,
            if provider.configured {
                "Credentials configured; model availability not guaranteed"
            } else {
                "Credentials missing; real runs unavailable"
            }
        ));
    }
    for (index, document) in report.documents.iter().enumerate() {
        ui.push_id(("checked_document", index), |ui| {
            document_check(ui, document, index + 1);
        });
    }
    if report.ready {
        ui.label("Inputs checked. The reviewed document text is frozen for this execution.");
    } else {
        ui.label("Resolve the input errors, then recheck. No inference has started.");
    }
}

fn document_check(ui: &mut Ui, document: &crate::preflight::DocumentCheck, number: usize) {
    if !document.error.is_empty() {
        ui.label(&document.error);
        return;
    }
    let snapshot = &document.snapshot;
    let included = snapshot.text.chars().count();
    ui.label(format!(
        "Document {number} · {} · Ready · {} of {} characters included",
        snapshot.name, included, snapshot.original_characters
    ));
    if snapshot.original_characters > included {
        ui.label("Document truncated at the 60,000-character extraction limit. Only the included text will be sent.");
    }
    crate::ui::theme::disclosure(
        ui,
        format!(
            "Inspect included text — Document {number} · {}",
            snapshot.name
        ),
    )
    .show(ui, |ui| {
        ui.label(&snapshot.text);
    });
}
