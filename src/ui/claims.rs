use crate::app::App;
use crate::model::Claim;
use egui::{RichText, Ui};

pub fn render_claims_view(ui: &mut Ui, app: &mut App) {
    crate::ui::theme::page_heading(
        ui,
        RichText::new("Claims & sources").color(app.theme.accent),
    );
    ui.label("These labels describe the debate, not independent verification. An unchallenged claim was simply not challenged. Excerpt matching establishes text occurrence in supplied material, not truth.");
    ui.add(
        egui::TextEdit::singleline(&mut app.claim_search)
            .id_salt("claim_search")
            .hint_text("Find a claim, author, or debate outcome"),
    );
    if app.claims.is_empty() {
        ui.label(if app.is_running { "No claims extracted yet. Claims appear after a stage finishes." } else { "No claims were extracted from the completed arguments. Read the arguments to inspect the reasoning." });
        return;
    }
    let visible = app.visible_claims();
    let mut open = None;
    for idx in &visible {
        ui.push_id(app.claims[*idx].origin_key(), |ui| {
            if let Some(action) = claim_card(ui, app, *idx) {
                open = Some(action);
            }
        });
        ui.add_space(12.0);
    }
    if visible.is_empty() {
        ui.label("No claims match this search. Clear the search to show all claims.");
    }
    if let Some((index, focus)) = open {
        app.inspect_claim(index);
        app.source_return_focus = Some(focus);
    }
}

fn claim_card(ui: &mut Ui, app: &mut App, idx: usize) -> Option<(usize, egui::Id)> {
    let claim = app.claims[idx].clone();
    let mut open = None;
    crate::ui::theme::card(&app.theme).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let compact = ui.available_width() < 400.0;
        let attribution = format!("Stage {} · {}", claim.turn_index, claim.author_model);
        ui.horizontal_wrapped(|ui| {
            let choice = ui.selectable_label(idx == app.selected_claim, RichText::new(&claim.id).strong());
            if app.selection_scroll == Some(crate::model::Tab::Claims) && idx == app.selected_claim {
                choice.request_focus();
                choice.scroll_to_me_animation(Some(egui::Align::Center), egui::style::ScrollAnimation::none());
            }
            if choice.clicked() { app.selected_claim = idx; }
            if !compact { ui.label(&attribution); }
        });
        if compact { ui.add(egui::Label::new(&attribution).wrap()); }
        ui.label(clean_claim_text(&claim.text));
        ui.label(format!("Source relationship: {}", provenance(&claim)));
        ui.label(format!("Parsed debate outcome: {}", claim.verdict.as_str()));
        if !claim.ambiguous_references.is_empty() { ui.label("Later references use a reused claim label. Their outcome has not been assigned to this statement."); }
        if !claim.attackers.is_empty() { ui.label(format!("Referenced by critiques from {}", claim.attackers.join(", "))); }
        let scope = egui::Id::unique(("claim_inspect", app.workspace_generation, claim.origin_key()));
        let inspect = ui.scope_builder(egui::UiBuilder::new().scope_id(scope), |ui| ui.button("Inspect statement, challenges and response")).inner;
        super::restore_source_focus(ui, &mut app.pending_focus, &inspect);
        if inspect.clicked() { open = Some((idx, inspect.id)); }
    });
    open
}

pub fn clean_claim_text(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.replace("**", "")
        .trim()
        .trim_start_matches([':', '-', ' '])
        .trim()
        .to_owned()
}

pub fn provenance(claim: &Claim) -> &'static str {
    let text = claim.text.to_uppercase();
    if text.contains("[SOURCE EXCERPT MATCHED]") {
        "Excerpt found in supplied material; not fact-checked"
    } else if text.contains("[ASSUMPTION") {
        "Model assumption; source not established"
    } else if text.contains("[JUDGEMENT") {
        "Model judgment; not external evidence"
    } else {
        "Not established"
    }
}
