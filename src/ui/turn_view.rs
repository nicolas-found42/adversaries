use crate::app::App;
use crate::model::TurnStatus;
use crate::ui::reader;
use egui::{RichText, Ui};

pub fn render_turn_view(ui: &mut Ui, app: &mut App) {
    if app.source_claim.is_some() {
        source_inspection(ui, app);
        if app.source_claim.is_none() {
            return;
        }
    } else {
        chronology(ui, app);
    }
    let Some(turn) = app.turns.get(app.selected_turn) else {
        reader::placeholder(
            ui,
            "Preparing the first stage",
            "Completed arguments will appear here. No brief is available yet.",
        );
        return;
    };
    let heading =
        crate::ui::theme::heading(ui, format!("Reading: stage {} · {}", turn.index, turn.role));
    ui.label(RichText::new(&turn.model).color(app.theme.text_muted));
    if turn.status == TurnStatus::Running {
        ui.spinner();
        ui.label(
            "This stage is working. You can read completed arguments without interrupting it.",
        );
        scroll_to_reading(&heading, app);
        return;
    }
    if matches!(turn.status, TurnStatus::Failed | TurnStatus::Incomplete) {
        ui.colored_label(
            app.theme.error,
            "Incomplete stage output. It is excluded from continuation and from claims.",
        );
    }
    crate::ui::theme::disclosure(ui, "Stage provenance and technical details").show(ui, |ui| {
        ui.label(format!(
            "{} · {:.0}s · {} words",
            turn.model,
            turn.seconds,
            turn.word_count()
        ));
        if ui.button("Copy this argument").clicked() {
            let practice = app.demo_mode
                || app
                    .last_config
                    .as_ref()
                    .is_some_and(|config| config.dry_run);
            ui.ctx()
                .copy_text(crate::brief::argument_export(turn, practice));
        }
        ui.label(
            turn.diagnostics["message"]
                .as_str()
                .unwrap_or("No additional diagnostic message."),
        );
    });
    reader::document(
        ui,
        ("turn", &app.current_question, turn.index),
        &turn.text,
        &mut app.markdown_cache,
    );
    scroll_to_reading(&heading, app);
}

fn scroll_to_reading(heading: &egui::Response, app: &App) {
    if app.selection_scroll == Some(crate::model::Tab::Debate) {
        heading
            .scroll_to_me_animation(Some(egui::Align::TOP), egui::style::ScrollAnimation::none());
    }
}

fn chronology(ui: &mut Ui, app: &mut App) {
    crate::ui::theme::disclosure(ui, "Stage sequence — choose an argument")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                crate::ui::theme::horizontal_group(ui, "Argument stages");
                for (idx, turn) in app.turns.iter().enumerate() {
                    let status = match turn.status {
                        TurnStatus::Complete => "Complete",
                        TurnStatus::Running => "Working",
                        TurnStatus::Queued => "Waiting",
                        _ => "Interrupted",
                    };
                    let label = format!("{} · {} · {status}", turn.index, turn.role);
                    if ui
                        .selectable_label(idx == app.selected_turn, label)
                        .clicked()
                    {
                        app.selected_turn = idx;
                        app.follow_live = false;
                        app.source_claim = None;
                    }
                }
            });
        });
}

fn source_inspection(ui: &mut Ui, app: &mut App) {
    let Some(index) = app.source_claim else {
        return;
    };
    let Some(claim) = app.claims.get(index).cloned() else {
        return;
    };
    let back = ui.button(format!("< Back to {}", app.source_return.title()));
    if back.clicked() {
        app.return_from_source();
        return;
    }
    statement_comparison(ui, app, &claim);
    if app.source_jump {
        back.request_focus();
        back.scroll_to_me(Some(egui::Align::TOP));
        app.source_jump = false;
    }
    supplied_material(ui, app, &claim);
    if !claim.ambiguous_references.is_empty() {
        let stages = claim
            .ambiguous_references
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        ui.label(format!("Ambiguous references in stages {stages}: this label identifies multiple statements. No later outcome is attributed without a unique origin."));
    }
}

fn statement_comparison(ui: &mut Ui, app: &App, claim: &crate::model::Claim) {
    let references = crate::claims::references(&app.turns, claim);
    if ui.available_width() >= 850.0 && !references.is_empty() {
        ui.columns(2, |columns| {
            for column in columns.iter_mut() {
                column.set_width(column.available_width());
            }
            columns[0].with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                selected_statement(ui, app, claim);
            });
            columns[1].with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                compared_statement(ui, claim, &references);
            });
        });
    } else {
        selected_statement(ui, app, claim);
        if !references.is_empty() {
            compared_statement(ui, claim, &references);
        }
    }
}

fn selected_statement(ui: &mut Ui, app: &App, claim: &crate::model::Claim) {
    egui::Frame::new()
        .fill(app.theme.accent.gamma_multiply(0.12))
        .inner_margin(16)
        .show(ui, |ui| {
            ui.strong(format!(
                "{} · originating statement · stage {}",
                claim.id, claim.turn_index
            ));
            let exact = app
                .turns
                .iter()
                .find(|t| t.index == claim.turn_index)
                .and_then(|t| t.text.get(claim.source_start..claim.source_end));
            ui.label(exact.unwrap_or(&claim.text));
            ui.label(format!(
                "Source relationship: {}",
                super::claims::provenance(claim)
            ));
            ui.label(format!(
                "Parsed debate outcome: {} — not independent verification",
                claim.verdict.as_str()
            ));
        });
}

fn compared_statement(
    ui: &mut Ui,
    claim: &crate::model::Claim,
    references: &[crate::claims::ClaimReference<'_>],
) {
    // The earliest prior use helps compare a later version; otherwise show the latest later use.
    let counterpart = references
        .iter()
        .find(|reference| reference.turn.index < claim.turn_index)
        .unwrap_or_else(|| references.last().unwrap());
    crate::ui::theme::heading(ui, "Compare text using this claim label");
    ui.label("Same label does not establish a revision, agreement or support. These are separate recorded statements and references.");
    ui.strong(format!(
        "{} · stage {} · {} · {}",
        claim.id, counterpart.turn.index, counterpart.turn.role, counterpart.turn.model
    ));
    ui.add(egui::Label::new(counterpart.text).wrap());
    crate::ui::theme::disclosure(
        ui,
        format!("All other uses of {} ({})", claim.id, references.len()),
    )
    .show(ui, |ui| {
        for reference in references {
            ui.strong(format!(
                "Stage {} · {} · {}",
                reference.turn.index, reference.turn.role, reference.turn.model
            ));
            ui.add(egui::Label::new(reference.text).wrap());
        }
    });
}

fn supplied_material(ui: &mut Ui, app: &App, claim: &crate::model::Claim) {
    let excerpts = crate::provenance::supplied_excerpts(
        claim,
        &app.current_question,
        app.last_config.as_ref(),
    );
    for (index, excerpt) in excerpts.iter().enumerate() {
        ui.push_id((claim.origin_key(), "supplied", index), |ui| {
            let source_name = excerpt.document_number.map_or_else(|| excerpt.name.clone(), |number| format!("Document {number} · {}", excerpt.name));
            ui.strong(format!("Matched supplied excerpt — {source_name}"));
            ui.label(&excerpt.excerpt);
            ui.label("Text occurrence matched against the reviewed material. Neither the source nor this claim was independently fact-checked.");
            crate::ui::theme::disclosure(ui, format!("Inspect supplied material — {source_name}")).show(ui, |ui| { ui.label(&excerpt.source); });
        });
    }
    if claim.text.contains("[SOURCE EXCERPT MATCHED]") && excerpts.is_empty() {
        ui.label("The argument reports an excerpt match, but Council could not locate that excerpt in the retained material. Read the original argument; this label is not verification.");
    }
}
