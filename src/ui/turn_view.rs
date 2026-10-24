use crate::app::App;
use crate::model::TurnStatus;
use crate::ui::reader;
use egui::{RichText, Ui};

pub fn render_turn_view(ui: &mut Ui, app: &mut App) {
    let Some(turn) = app.turns.get(app.selected_turn) else {
        reader::placeholder(
            ui,
            "The council is getting ready.",
            "Each completed argument will be available here.",
        );
        return;
    };
    ui.label(RichText::new(&turn.role).size(22.0).strong());
    ui.label(RichText::new(&turn.model).color(app.theme.model_color(&turn.model)));
    ui.horizontal(|ui| {
        ui.weak(format!(
            "{:.0}s · {} words",
            app.elapsed(turn),
            turn.word_count()
        ));
        if !turn.text.is_empty() && ui.small_button("Copy argument").clicked() {
            ui.ctx().copy_text(turn.text.clone());
        }
    });
    ui.add_space(16.0);
    if turn.status == TurnStatus::Running {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Thinking through the argument…");
        });
        ui.weak(
            "Output appears when this model finishes. You can read earlier stages while it works.",
        );
    } else {
        if matches!(turn.status, TurnStatus::Failed | TurnStatus::Incomplete) {
            ui.colored_label(
                app.theme.error,
                "Incomplete · excluded from subsequent debate stages",
            );
            ui.label(
                turn.diagnostics["message"]
                    .as_str()
                    .unwrap_or("This stage stopped."),
            );
        }
        reader::document(
            ui,
            ("turn", &app.current_question, turn.index),
            &turn.text,
            &mut app.markdown_cache,
        );
    }
}
