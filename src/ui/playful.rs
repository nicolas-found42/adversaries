//! Explanatory motion uses real run state; decoration never implies agreement.
use super::sculpture::Sculpture;
use crate::{app::App, model::TurnStatus};
use egui::{RichText, Stroke, Ui};
use keyframe::{ease, functions::EaseInOut};

pub fn crew(ui: &mut Ui, app: &App, preview: bool) {
    let width = ui.available_width();
    let columns = if width < 250.0 { 2 } else { 4 };
    let height = if width < 450.0 { 130.0 } else { 210.0 };
    let row_height = if columns == 2 { height / 2.0 } else { height };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let (moving, time) = motion_clock(ui, app, rect);
    let active = app
        .turns
        .iter()
        .position(|t| t.status == TurnStatus::Running);
    let colors = [
        app.theme.accent,
        app.theme.mistral,
        app.theme.ollama,
        app.theme.opencode,
    ];
    for (i, name) in ["Idea", "Challenge", "Reply", "Brief"].iter().enumerate() {
        let x = rect.left() + width * ((i % columns) as f32 + 0.5) / columns as f32;
        let row_top = rect.top() + (i / columns) as f32 * row_height;
        let bob = ease(
            EaseInOut,
            -3.0_f32,
            3.0,
            f64::from((time * 0.6 + i as f32 * 0.2).sin() * 0.5 + 0.5),
        );
        let center = egui::pos2(
            x,
            row_top + row_height * 0.43 + if moving { bob } else { 0.0 },
        );
        let selected = !preview && active.is_some_and(|idx| role_group(&app.turns[idx].role) == i);
        let size = (width / columns as f32 - 8.0).clamp(24.0, (row_height - 28.0).min(180.0));
        avatar(
            ui,
            egui::Rect::from_center_size(center, egui::Vec2::splat(size)),
            Sculpture {
                role: i,
                color: colors[i],
                time,
            },
            selected,
        );
        ui.painter().text(
            egui::pos2(x, row_top + row_height - 16.0),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(if width < 360.0 { 13.0 } else { 16.0 }),
            app.theme.text,
        );
    }
}

fn motion_clock(ui: &Ui, app: &App, rect: egui::Rect) -> (bool, f32) {
    let clock = ui.ctx().input(|i| i.time);
    let born = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or_insert_with(ui.scope_id().with("crew_birth"), || clock));
    let moving = app.theme.motion_seconds > 0.0 && (app.is_running || clock - born < 3.0);
    let time = if moving { (clock - born) as f32 } else { 0.0 };
    if moving && ui.is_rect_visible(rect) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    (moving, time)
}

fn role_group(role: &str) -> usize {
    let role = role.to_lowercase();
    if role.contains("brief") {
        3
    } else if role.contains("rebuttal") {
        2
    } else {
        usize::from(role.contains("red") || role.contains("critique"))
    }
}

fn avatar(ui: &Ui, rect: egui::Rect, sculpture: Sculpture, speaking: bool) {
    let Sculpture { color, time, .. } = sculpture;
    let center = rect.center();
    let size = rect.width();
    if speaking {
        let radius = size * 0.44 + (time * 3.0).sin() * 2.0;
        ui.painter()
            .circle_stroke(center, radius, Stroke::new(2.0, color));
    }
    super::sculpture::paint(ui, rect, sculpture);
}

pub fn journey(ui: &mut Ui, app: &App) {
    let (step, message) = journey_state(app);
    egui::Frame::new()
        .fill(app.theme.card_bg)
        .corner_radius(14)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, label) in ["1 · Ask", "2 · Debate", "3 · Brief"].iter().enumerate() {
                    let color = if i == step {
                        app.theme.accent
                    } else {
                        app.theme.text_muted
                    };
                    ui.label(RichText::new(*label).color(color).strong());
                    if i < 2 {
                        ui.label("›");
                    }
                }
            });
            ui.add(egui::Label::new(message).wrap());
            if app.is_running {
                let speaking = app.turns.iter().find(|t| t.status == TurnStatus::Running);
                let activity = speaking.map_or("Getting the next turn ready…", |t| {
                    match role_group(&t.role) {
                        1 => "Challenge: looking for problems in the idea.",
                        2 => "Reply: answering the challenges.",
                        3 => "Brief: bringing the options and risks together.",
                        _ => "Idea: building the first suggestion.",
                    }
                });
                ui.label(RichText::new(activity).color(app.theme.highlight));
                crew(ui, app, false);
            }
        });
}

pub fn journey_state(app: &App) -> (usize, &'static str) {
    if app.is_running
        && app
            .turns
            .iter()
            .any(|t| t.status == TurnStatus::Running && role_group(&t.role) == 3)
    {
        (
            2,
            "Writing the brief: a short guide to the options and tradeoffs.",
        )
    } else if app.is_running {
        (
            1,
            "The models take turns: propose an idea, look for problems, then reply.",
        )
    } else if app.has_brief() {
        (
            2,
            "Your brief is ready. It is a model's suggestion. Check the facts before deciding.",
        )
    } else {
        (
            1,
            "The debate paused before a brief was ready. Read the completed arguments below.",
        )
    }
}

/// The same character and plain explanation follow a role into its argument.
pub fn role_caption(ui: &mut Ui, theme: &super::theme::Theme, role: &str) {
    let group = role_group(role);
    let (name, detail, color) = match group {
        1 => (
            "Challenge",
            "Looking for gaps, risks, and shaky assumptions.",
            theme.mistral,
        ),
        2 => (
            "Reply",
            "Answering the challenges and updating the idea.",
            theme.ollama,
        ),
        3 => (
            "Brief",
            "Bringing the choices and tradeoffs together.",
            theme.opencode,
        ),
        _ => (
            "Idea",
            "Making the first suggestion for everyone to test.",
            theme.accent,
        ),
    };
    super::theme::feature_card(theme, color).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 72.0), egui::Sense::hover());
            avatar(
                ui,
                rect,
                Sculpture {
                    role: group,
                    color,
                    time: 0.0,
                },
                false,
            );
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width());
                ui.label(RichText::new(name).strong().color(color));
                ui.add(egui::Label::new(detail).wrap());
            });
        });
    });
}
