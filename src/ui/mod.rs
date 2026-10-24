pub mod brief_view;
pub mod claims;
pub mod composer;
pub mod diagnostics;
pub mod help;
mod reader;
pub mod sessions;
pub mod theme;
pub mod timeline;
pub mod turn_view;

use crate::app::App;
use crate::model::Tab;
use egui::{Frame, Margin, RichText, Ui};

pub fn render(ui: &mut Ui, app: &mut App) {
    app.theme.apply(ui.ctx());
    egui::Panel::bottom("status").show(ui, |ui| footer(ui, app));
    egui::Panel::left("workspace_sidebar")
        .exact_size(232.0)
        .frame(
            Frame::new()
                .fill(egui::Color32::from_rgb(20, 21, 24))
                .inner_margin(18),
        )
        .show(ui, |ui| sidebar(ui, app));
    if app.inspector_open && !app.turns.is_empty() && ui.available_width() > 850.0 {
        egui::Panel::right("debate_inspector")
            .default_size(270.0)
            .size_range(230.0..=360.0)
            .frame(Frame::new().fill(app.theme.bg).inner_margin(18))
            .show(ui, |ui| timeline::render_timeline(ui, app));
    }
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(app.theme.bg)
                .inner_margin(Margin::same(28)),
        )
        .show(ui, |ui| workspace(ui, app));
    if app.composer_open {
        composer::show_modal(ui.ctx(), app);
    }
}

fn sidebar(ui: &mut Ui, app: &mut App) {
    ui.add_space(8.0);
    ui.label(RichText::new("C  /  COUNCIL").size(17.0).strong());
    ui.label(
        RichText::new("A clearer decision.")
            .small()
            .color(app.theme.text_muted),
    );
    ui.add_space(24.0);
    if ui
        .add_sized(
            [ui.available_width(), 38.0],
            egui::Button::new("+  New decision"),
        )
        .clicked()
    {
        app.composer_open = true;
    }
    ui.add_space(22.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("YOUR DECISIONS")
                .small()
                .color(app.theme.text_muted),
        );
        if ui.small_button("Refresh").clicked() {
            app.refresh_sessions();
        }
    });
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt("sidebar_history")
        .max_height((ui.available_height() - 130.0).max(100.0))
        .show(ui, |ui| {
            let sessions = app.sessions.clone();
            for session in &sessions {
                let selected = session
                    .file_name
                    .rsplit_once('.')
                    .is_some_and(|(stem, _)| stem == app.session_key);
                let title: String = session.question.chars().take(65).collect();
                let response = ui.add_enabled(
                    !app.is_running,
                    egui::Button::selectable(selected, title)
                        .wrap()
                        .min_size(egui::vec2(ui.available_width(), 50.0)),
                );
                if response.on_hover_text(&session.question).clicked() {
                    app.load_session_by_path(&session.path_str);
                }
                ui.label(
                    RichText::new(session.created_at.get(..10).unwrap_or(&session.created_at))
                        .small()
                        .color(app.theme.text_muted),
                );
                ui.add_space(8.0);
            }
            if sessions.is_empty() {
                ui.weak("Your decisions will appear here.");
            }
        });
    ui.add_space(20.0);
    if ui.button("All decisions").clicked() {
        app.active_tab = Tab::Sessions;
    }
    if ui.button("Help & shortcuts").clicked() {
        app.active_tab = Tab::Help;
    }
    ui.label(
        RichText::new("Private workspace · saved locally")
            .small()
            .color(app.theme.text_muted),
    );
}

fn workspace(ui: &mut Ui, app: &mut App) {
    if app.current_question.is_empty() {
        welcome(ui, app);
        return;
    }
    workspace_actions(ui, app);
    ui.add_space(12.0);
    ui.label(RichText::new(&app.current_question).size(24.0).strong());
    ui.add_space(12.0);
    workspace_tabs(ui, app);
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(12.0);
    if let Some(error) = app.error.clone() {
        error_card(ui, app, &error);
    }
    match app.active_tab {
        Tab::Brief => brief_view::render_brief_view(ui, app),
        Tab::Debate => turn_view::render_turn_view(ui, app),
        Tab::Claims => claims::render_claims_view(ui, app),
        Tab::Sessions => sessions::render_sessions_view(ui, app),
        Tab::Help => help::render_help_view(ui, &app.theme),
        Tab::Diagnostics => diagnostics::render(ui, app),
    }
}

fn workspace_actions(ui: &mut Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("DECISION WORKSPACE")
                .small()
                .color(app.theme.text_muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.toggle_value(&mut app.inspector_open, "Debate panel");
            if app.is_running {
                if ui.button("Stop").clicked() {
                    app.cancel_run();
                }
            } else if ui.button("Run again").clicked() {
                app.rerun_current();
            }
        });
    });
}

fn workspace_tabs(ui: &mut Ui, app: &mut App) {
    ui.horizontal(|ui| {
        for (tab, title) in [
            (Tab::Brief, "Decision brief"),
            (Tab::Debate, "Debate"),
            (Tab::Claims, "Claims & assumptions"),
            (Tab::Diagnostics, "Diagnostics"),
        ] {
            if ui.selectable_label(app.active_tab == tab, title).clicked() {
                app.active_tab = tab;
            }
        }
        if app.is_running {
            ui.checkbox(&mut app.follow_live, "Follow live");
        }
        if !app.brief_text.is_empty()
            && app.active_tab != Tab::Brief
            && ui.button("Brief ready ").clicked()
        {
            app.active_tab = Tab::Brief;
        }
    });
}

fn welcome(ui: &mut Ui, app: &mut App) {
    if app.active_tab == Tab::Sessions {
        sessions::render_sessions_view(ui, app);
        return;
    }
    if app.active_tab == Tab::Help {
        help::render_help_view(ui, &app.theme);
        return;
    }
    ui.add_space((ui.available_height() * 0.16).min(100.0));
    ui.label(
        RichText::new("THINK IT THROUGH")
            .small()
            .color(app.theme.accent),
    );
    ui.add_space(14.0);
    ui.label(
        RichText::new("One decision.\nMore than one perspective.")
            .size(36.0)
            .strong(),
    );
    ui.add_space(16.0);
    ui.label(RichText::new("Bring a question. Choose your council. Let competing arguments\nsharpen a recommendation you can act on.").size(17.0).color(app.theme.text_muted));
    ui.add_space(28.0);
    if ui
        .add_sized([190.0, 44.0], egui::Button::new("Start a decision  "))
        .clicked()
    {
        app.composer_open = true;
    }
    ui.add_space(40.0);
    let steps = [
        ("01", "Frame the question", "Add context and constraints."),
        (
            "02",
            "Challenge the reasoning",
            "Choose a model for every role.",
        ),
        ("03", "Read the decision brief", "Tradeoffs and next steps."),
    ];
    ui.columns(3, |columns| {
        for (column, (number, title, detail)) in columns.iter_mut().zip(steps) {
            Frame::new()
                .fill(app.theme.card_bg)
                .corner_radius(10)
                .inner_margin(16)
                .show(column, |ui| {
                    ui.set_min_height(100.0);
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(number).small().color(app.theme.accent));
                    ui.strong(title);
                    ui.weak(detail);
                });
        }
    });
}

fn error_card(ui: &mut Ui, app: &mut App, error: &str) {
    Frame::new()
        .fill(app.theme.error.gamma_multiply(0.09))
        .stroke(egui::Stroke::new(1.0, app.theme.error.gamma_multiply(0.4)))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.label(
                RichText::new("This stage could not finish")
                    .strong()
                    .color(app.theme.error),
            );
            ui.label(error);
            ui.weak("Your completed arguments are still available.");
            ui.horizontal(|ui| {
                if app.last_config.is_some() && ui.button("Retry this stage").clicked() {
                    app.retry_stage();
                }
                if ui.button("Edit council").clicked() {
                    app.composer_open = true;
                }
            });
        });
    ui.add_space(16.0);
}

fn footer(ui: &mut Ui, app: &App) {
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        if app.is_running {
            ui.spinner();
        } else {
            ui.label(
                RichText::new(if app.error.is_some() {
                    "Stopped"
                } else {
                    "Ready"
                })
                .color(if app.error.is_some() {
                    app.theme.error
                } else {
                    app.theme.success
                }),
            );
        }
        let message = if app.error.is_some() {
            "Debate stopped · completed stages preserved"
        } else {
            &app.status_message
        };
        ui.label(RichText::new(message).small().color(app.theme.text_muted));
    });
}
