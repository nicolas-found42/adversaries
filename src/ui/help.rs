use egui::{Grid, RichText, ScrollArea, Ui};

use crate::ui::theme::Theme;

pub fn render_help_view(ui: &mut Ui, theme: &Theme) {
    ui.heading(RichText::new("Help & Keys").color(theme.accent));
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.label(RichText::new("Navigation Shortcuts").strong().color(theme.highlight));
        Grid::new("help_keys").num_columns(2).show(ui, |ui| {
            key_row(ui, theme, "1 .. 6", "Switch tabs (1 Debate, 2 Brief, 3 Claims, 4 History, 5 Help, 6 Diagnostics)");
            key_row(ui, theme, "↑ / ↓ / j / k", "Navigate turns, claims, or sessions");
            key_row(ui, theme, "Tab", "Cycle views (when no text field is focused)");
            key_row(ui, theme, "Enter", "In History: load the selected past session");
            ui.end_row();
        });
        ui.add_space(8.0);

        ui.label(RichText::new("Council Actions").strong().color(theme.highlight));
        Grid::new("help_actions").num_columns(2).show(ui, |ui| {
            key_row(ui, theme, "n", "Open the New Council composer");
            key_row(ui, theme, "c", "Cancel the currently running debate");
            key_row(ui, theme, "r", "Rerun the current debate question");
            key_row(ui, theme, "Esc", "Close the composer");
            #[cfg(not(target_arch = "wasm32"))]
            key_row(ui, theme, "q", "Quit the app");
            #[cfg(target_arch = "wasm32")]
            key_row(ui, theme, "q", "(Quit is native-only in the browser build)");
            ui.end_row();
        });
        ui.add_space(8.0);

        ui.label(RichText::new("Adversarial Council Protocol").strong().color(theme.highlight));
        Grid::new("help_protocol").num_columns(2).show(ui, |ui| {
            protocol_row(ui, "Turn 1: Opening", theme.mistral, "Lead Analyst opening position, numbers propositions C1..CN");
            protocol_row(ui, "Turn 2: Critic 1", theme.openrouter, "Challenges the opening claims using its chosen provider/model");
            protocol_row(ui, "Turn 3: Critic 2", theme.ollama, "Challenges the preceding critique");
            protocol_row(ui, "Turn 4: Critic 3", theme.opencode, "Challenges the preceding critique");
            protocol_row(ui, "Turn 5: Rebuttal", theme.mistral, "Concedes, partially concedes, or rebuts; updates recommendation");
            protocol_row(ui, "Turn 6: Brief", theme.mistral, "Synthesizes the Executive Decision Brief with full audit trail");
            ui.end_row();
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "One UI, two surfaces: this same egui app runs natively (cargo run) and in the browser (trunk serve) with live reload while you edit the Rust views.",
            )
            .color(theme.text_muted),
        );
    });
}

fn key_row(ui: &mut Ui, theme: &Theme, keys: &str, description: &str) {
    ui.label(
        RichText::new(format!("  {keys:<18}"))
            .strong()
            .color(theme.highlight),
    );
    ui.label(description);
    ui.end_row();
}

fn protocol_row(ui: &mut Ui, label: &str, color: egui::Color32, description: &str) {
    ui.label(
        RichText::new(format!("  {label:<22}"))
            .strong()
            .color(color),
    );
    ui.label(description);
    ui.end_row();
}
