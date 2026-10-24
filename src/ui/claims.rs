use crate::app::App;
use crate::model::Claim;
use egui::{Frame, RichText, ScrollArea, Ui};

pub fn render_claims_view(ui: &mut Ui, app: &mut App) {
    ui.label(RichText::new("Claims & assumptions").size(22.0).strong());
    ui.weak("Labels describe model reasoning, not independently verified facts. A matched excerpt confirms the quote exists in your input; it does not verify the conclusion.");
    ui.add_space(12.0);
    if app.claims.is_empty() {
        ui.weak("Numbered claims appear here as each argument completes.");
        return;
    }
    ScrollArea::vertical()
        .id_salt(("claims", &app.current_question))
        .show(ui, |ui| {
            for (idx, claim) in app.claims.iter().enumerate() {
                Frame::new()
                    .fill(app.theme.card_bg)
                    .corner_radius(8)
                    .inner_margin(16)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .selectable_label(
                                    idx == app.selected_claim,
                                    RichText::new(&claim.id).strong(),
                                )
                                .clicked()
                            {
                                app.selected_claim = idx;
                            }
                            ui.label(
                                RichText::new(provenance(claim))
                                    .small()
                                    .color(app.theme.accent),
                            );
                            ui.label(
                                RichText::new(claim.verdict.as_str())
                                    .small()
                                    .color(app.theme.warning),
                            );
                        });
                        ui.label(&claim.text);
                        ui.label(
                            RichText::new(format!(
                                "Stage {} · {}",
                                claim.turn_index, claim.author_model
                            ))
                            .small()
                            .color(app.theme.text_muted),
                        );
                        if !claim.attackers.is_empty() {
                            ui.weak(format!("Referenced by: {}", claim.attackers.join(", ")));
                        }
                        if ui.small_button("Read source argument").clicked() {
                            if let Some(index) =
                                app.turns.iter().position(|t| t.index == claim.turn_index)
                            {
                                app.selected_turn = index;
                                app.active_tab = crate::model::Tab::Debate;
                                app.follow_live = false;
                            }
                        }
                    });
                ui.add_space(6.0);
            }
        });
}

fn provenance(claim: &Claim) -> &'static str {
    let text = claim.text.to_uppercase();
    if text.contains("[SOURCE EXCERPT MATCHED]") {
        "SOURCE EXCERPT MATCHED"
    } else if text.contains("[ASSUMPTION") {
        "MODEL ASSUMPTION"
    } else if text.contains("[JUDGEMENT") {
        "MODEL JUDGEMENT"
    } else {
        "UNCLASSIFIED"
    }
}
