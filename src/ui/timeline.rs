use crate::app::App;
use crate::model::{Tab, TurnStatus};
use egui::{Frame, RichText, ScrollArea, Ui};

pub fn render_timeline(ui: &mut Ui, app: &mut App) {
    ui.add_space(8.0);
    ui.label(
        RichText::new("THE COUNCIL")
            .small()
            .color(app.theme.text_muted),
    );
    let complete = app
        .turns
        .iter()
        .filter(|t| t.status == TurnStatus::Complete)
        .count();
    ui.label(
        RichText::new(format!("{complete} of {} stages complete", app.total_calls))
            .size(18.0)
            .strong(),
    );
    ui.add(
        egui::ProgressBar::new(complete as f32 / app.total_calls.max(1) as f32).desired_height(4.0),
    );
    ui.add_space(10.0);
    ScrollArea::vertical()
        .id_salt("council_timeline")
        .auto_shrink(false)
        .show(ui, |ui| {
            for idx in 0..app.turns.len() {
                card(ui, app, idx);
            }
        });
}

fn card(ui: &mut Ui, app: &mut App, idx: usize) {
    let turn = &app.turns[idx];
    let selected = idx == app.selected_turn && app.active_tab == Tab::Debate;
    let (label, color) = match turn.status {
        TurnStatus::Running => ("THINKING", app.theme.warning),
        TurnStatus::Complete => ("COMPLETE", app.theme.success),
        TurnStatus::Incomplete => ("INCOMPLETE", app.theme.error),
        TurnStatus::Failed => ("STOPPED", app.theme.error),
        TurnStatus::Queued => ("QUEUED", app.theme.text_muted),
    };
    let elapsed = app.elapsed(turn);
    let response = Frame::new()
        .fill(if selected {
            app.theme.card_bg
        } else {
            app.theme.bg
        })
        .stroke(egui::Stroke::new(
            1.0,
            if selected {
                app.theme.border_focus
            } else {
                app.theme.border
            },
        ))
        .corner_radius(8)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let response = ui.add(
                egui::Button::selectable(selected, format!("{:02}  {}", turn.index, turn.role))
                    .wrap(),
            );
            ui.label(
                RichText::new(&turn.model)
                    .small()
                    .color(app.theme.model_color(&turn.model)),
            );
            ui.label(
                RichText::new(format!("{label} · {elapsed:.0}s"))
                    .small()
                    .color(color),
            );
            response
        });
    if response.inner.clicked() || response.response.interact(egui::Sense::click()).clicked() {
        app.selected_turn = idx;
        app.active_tab = Tab::Debate;
        app.follow_live = false;
    }
    ui.add_space(4.0);
}
