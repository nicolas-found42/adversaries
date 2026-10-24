use crate::app::App;
use crate::ui::reader;
use egui::{RichText, Ui};

pub fn render_brief_view(ui: &mut Ui, app: &mut App) {
    if app.brief_text.trim().is_empty() {
        reader::placeholder(
            ui,
            "A recommendation, earned through debate.",
            "Your brief will appear when the council finishes. You can read each completed argument in Debate.",
        );
        if app.is_running {
            ui.spinner();
        }
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("EXECUTIVE BRIEF")
                .small()
                .color(app.theme.text_muted),
        );
        if ui.button("Copy brief").clicked() {
            ui.ctx().copy_text(app.brief_text.clone());
        }
    });
    ui.add_space(8.0);
    reader::document(
        ui,
        ("brief", &app.current_question),
        &app.brief_text,
        &mut app.markdown_cache,
    );
}
