pub mod brief_view;
pub mod claims;
pub mod composer;
pub mod diagnostics;
mod execution;
pub mod help;
pub(crate) mod reader;
pub mod sessions;
mod table;
pub mod theme;
pub mod timeline;
pub mod turn_view;

use crate::app::App;
use crate::model::Tab;
use egui::{Frame, Margin, RichText, Ui};

/// Below this width the debate side panel is not shown, so nothing should offer
/// a control that would only toggle hidden content.
pub const INSPECTOR_MIN_WIDTH: f32 = 850.0;

/// One place decides whether the debate panel fits. Measured before the panel
/// takes its width, so the control that closes it stays reachable while open.
pub fn panel_room(ui: &Ui) -> bool {
    ui.available_width() >= INSPECTOR_MIN_WIDTH
}

/// Shared identity for canvas and HTML reader scroll ownership.
pub fn workspace_key(app: &App) -> egui::Id {
    let source = app
        .source_claim
        .and_then(|index| app.claims.get(index))
        .map(crate::model::Claim::origin_key);
    egui::Id::unique((
        "workspace",
        app.workspace_generation,
        &app.current_question,
        app.active_tab,
        source,
    ))
}

pub fn render(ui: &mut Ui, app: &mut App) {
    render_with_workspace(ui, app);
}

/// Returns the identity of the content actually drawn, even if a click changes state mid-pass.
pub fn render_with_workspace(ui: &mut Ui, app: &mut App) -> egui::Id {
    reader::document::begin_frame(ui.ctx());
    let rendered_tab = app.active_tab;
    app.theme.apply(ui.ctx());
    egui::Panel::bottom("status").show(ui, |ui| footer(ui, app));
    egui::Panel::top("navigation").show(ui, |ui| navigation(ui, app));
    let rendered_workspace = workspace_key(app);
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(app.theme.bg)
                .inner_margin(Margin::same(20)),
        )
        .show(ui, |ui| {
            theme::region(ui, egui::accesskit::Role::Main, "Decision workspace");
            egui::ScrollArea::vertical()
                .id_salt(rendered_workspace)
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(1000.0));
                    workspace(ui, app, false);
                });
        });
    if app.composer_open {
        composer::show_modal(ui.ctx(), app);
    }
    execution::show(ui.ctx(), app);
    if app.active_tab == rendered_tab
        && app.selection_scroll == Some(rendered_tab)
        && !ui.ctx().will_discard()
        && !ui.ctx().input(egui::InputState::is_scrolling)
    {
        app.selection_scroll = None;
    }
    crate::accessibility::order_landmarks(ui);
    rendered_workspace
}

fn restore_source_focus(ui: &Ui, pending: &mut Option<egui::Id>, response: &egui::Response) {
    if *pending != Some(response.id) {
        return;
    }
    let visible = ui.clip_rect().contains(response.rect.center());
    if !visible {
        response.scroll_to_me_animation(
            Some(egui::Align::Center),
            egui::style::ScrollAnimation::none(),
        );
    }
    response.request_focus();
    if visible && !ui.ctx().will_discard() {
        *pending = None;
    }
}

fn navigation(ui: &mut Ui, app: &mut App) {
    theme::region(ui, egui::accesskit::Role::Navigation, "Council navigation");
    if ui.available_width() < 500.0 {
        ui.horizontal(|ui| {
            theme::horizontal_group(ui, "Council controls");
            ui.label(RichText::new("Council").strong());
            ui.menu_button("Navigate", |ui| {
                if navigation_items(ui, app) {
                    ui.close();
                }
            });
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            theme::horizontal_group(ui, "Council controls");
            ui.label(RichText::new("C / COUNCIL").strong());
            navigation_items(ui, app);
        });
    }
}

fn navigation_items(ui: &mut Ui, app: &mut App) -> bool {
    let mut changed = false;
    if ui.button("New decision").clicked() {
        app.composer_open = true;
        changed = true;
    }
    if ui
        .selectable_label(app.active_tab == Tab::Sessions, "Decisions")
        .clicked()
    {
        app.active_tab = Tab::Sessions;
        app.refresh_sessions();
        changed = true;
    }
    if app.has_workspace()
        && matches!(app.active_tab, Tab::Sessions | Tab::Help)
        && ui.button("Current decision").clicked()
    {
        app.active_tab = if app.has_brief() {
            Tab::Brief
        } else {
            Tab::Debate
        };
        changed = true;
    }
    if ui
        .selectable_label(app.active_tab == Tab::Help, "Help")
        .clicked()
    {
        app.active_tab = Tab::Help;
        changed = true;
    }
    text_size_control(ui);
    changed
}

fn text_size_control(ui: &mut Ui) {
    egui::ComboBox::from_id_salt("text_scale")
        .selected_text("Text size")
        .show_ui(ui, |ui| {
            for (label, scale) in [("Standard", 1.0), ("Large", 1.25), ("Extra large", 1.5)] {
                if ui.button(label).clicked() {
                    ui.ctx().set_zoom_factor(scale);
                    ui.close();
                }
            }
        });
}

pub(crate) fn short_question(question: &str) -> String {
    const LIMIT: usize = 46;
    let first_line = question.lines().next().unwrap_or(question).trim();
    if first_line.chars().count() <= LIMIT {
        return first_line.to_string();
    }
    let cut: String = first_line.chars().take(LIMIT).collect();
    match cut.rsplit_once(' ') {
        Some((head, _)) => format!("{head}…"),
        None => format!("{cut}…"),
    }
}

fn workspace(ui: &mut Ui, app: &mut App, room_for_panel: bool) {
    if !matches!(app.active_tab, Tab::Sessions | Tab::Help) {
        launch_failure_card(ui, app);
    }
    if !app.has_workspace() && !matches!(app.active_tab, Tab::Sessions | Tab::Help) {
        if app.load_failed() {
            load_failed_card(ui, app);
        } else {
            welcome(ui, app);
        }
        return;
    }
    if matches!(app.active_tab, Tab::Sessions | Tab::Help) {
        active_view(ui, app);
        return;
    }
    decision_question(ui, &app.current_question);
    ui.add_space(12.0);
    workspace_actions(ui, app, room_for_panel);
    ui.add_space(12.0);
    workspace_tabs(ui, app);
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(12.0);
    let recovery = workspace_notice(ui, app);
    if !recovery || app.active_tab != Tab::Brief || app.has_brief() {
        active_view(ui, app);
    }
}

fn decision_question(ui: &mut Ui, question: &str) {
    let question = if question.trim().is_empty() {
        "Question not recorded"
    } else {
        question
    };
    let shortened = question.chars().count() > 160;
    let title = if shortened {
        format!("{}…", question.chars().take(160).collect::<String>())
    } else {
        question.to_owned()
    };
    let heading = ui.add(egui::Label::new(RichText::new(&title).size(24.0).strong()).wrap());
    theme::mark_heading(ui, &heading, &title, 1);
    if shortened {
        crate::ui::theme::disclosure(ui, "Read full decision question").show(ui, |ui| {
            ui.label(question);
        });
    }
}

/// Exactly one notice explains the current state: a real failure, an unfinished
/// saved decision, or nothing at all.
fn workspace_notice(ui: &mut Ui, app: &mut App) -> bool {
    if app.load_failed() {
        load_failed_card(ui, app);
    } else if app.stopped_by_user && app.has_brief() {
        error_card(
            ui,
            app,
            "Stopped by you after a brief was generated. Its final save may not have completed.",
        );
    } else if let Some(error) = app.error.clone() {
        error_card(ui, app, &error);
    } else if app.stopped_by_user || app.needs_brief() {
        unfinished_card(ui, app);
    } else {
        return false;
    }
    true
}

fn active_view(ui: &mut Ui, app: &mut App) {
    match app.active_tab {
        Tab::Brief => brief_view::render_brief_view(ui, app),
        Tab::Debate => turn_view::render_turn_view(ui, app),
        Tab::Claims => claims::render_claims_view(ui, app),
        Tab::Sessions => sessions::render_sessions_view(ui, app),
        Tab::Help => help::render_help_view(ui, &app.theme),
        Tab::Diagnostics => diagnostics::render(ui, app),
    }
}

fn workspace_actions(ui: &mut Ui, app: &mut App, _room_for_panel: bool) {
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("Decision actions", |ui| {
            if !app.is_running && ui.button("Review a new run").clicked() {
                app.rerun_current();
                ui.close();
            }
            if ui.button("Run details").clicked() {
                app.active_tab = Tab::Diagnostics;
                ui.close();
            }
        });
        if app.is_running && ui.button("Stop run").on_hover_text("Stop requesting further work. In-flight provider requests may still be charged.").clicked() {
            app.cancel_run();
        }
    });
    if app.is_running {
        ui.label(
            if app.demo_mode
                || app
                    .last_config
                    .as_ref()
                    .is_some_and(|config| config.dry_run)
            {
                "Practice example · No inference charges."
            } else {
                "Requests already sent may still be charged if you stop."
            },
        );
        let complete = app
            .turns
            .iter()
            .filter(|t| t.status == crate::model::TurnStatus::Complete)
            .count();
        ui.label(format!("{complete} of {} stages complete", app.total_calls));
        if let Some(turn) = app
            .turns
            .iter()
            .find(|t| t.status == crate::model::TurnStatus::Running)
        {
            ui.label(format!(
                "Now: stage {} · {} · {}",
                turn.index, turn.role, turn.model
            ));
        }
    }
}

fn workspace_tabs(ui: &mut Ui, app: &mut App) {
    ui.horizontal_wrapped(|ui| {
        theme::horizontal_group(ui, "Decision views");
        for (tab, title) in [
            (Tab::Brief, "Decision brief"),
            (Tab::Debate, "Arguments"),
            (Tab::Claims, "Claims & sources"),
        ] {
            if ui.selectable_label(app.active_tab == tab, title).clicked() {
                app.active_tab = tab;
                app.source_claim = None;
            }
        }
        if app.is_running {
            ui.checkbox(&mut app.follow_live, "Follow latest stage");
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
    // A dedicated salt so the welcome scroll position cannot leak into another
    // view's scroll area, which shares egui's default id.
    ui.push_id("welcome", |ui| {
        welcome_content(ui, app);
        // Room after the last card so the narrow layout never cuts one off.
        ui.add_space(24.0);
    });
}

fn welcome_content(ui: &mut Ui, app: &mut App) {
    // On a short window the generous leading gap would push every step card past
    // the fold, so the page looked blank instead of scrollable.
    let short = ui.available_height() < 620.0;
    let (top_gap, gap) = if short { (24.0, 16.0) } else { (100.0, 34.0) };
    ui.add_space((ui.available_height() * 0.16).min(top_gap));
    ui.label(
        RichText::new("ADVERSARIAL COUNCIL")
            .small()
            .color(app.theme.accent),
    );
    ui.add_space(14.0);
    let title = ui.add(
        egui::Label::new(
            RichText::new("A decision, argued from several sides.")
                .size(34.0)
                .strong(),
        )
        .wrap(),
    );
    theme::mark_heading(ui, &title, "A decision, argued from several sides.", 1);
    ui.add_space(14.0);
    if ui
        .add_sized(
            [ui.available_width().min(230.0), 48.0],
            egui::Button::new(RichText::new("Start a decision").strong())
                .fill(app.theme.accent.gamma_multiply(0.28))
                .stroke(egui::Stroke::new(1.5, app.theme.accent)),
        )
        .clicked()
    {
        app.composer_open = true;
    }
    ui.add_space(14.0);
    ui.add(
        egui::Label::new(
            RichText::new(
                "Write the question you are facing. The council argues it, then gives you a brief with a recommendation and the tradeoffs behind it.",
            )
            .size(18.0)
            .color(app.theme.text_muted),
        )
        .wrap(),
    );
    ui.add_space(gap);
    let steps = [
        (
            "01",
            "Write the question",
            "One decision, with your options and deadline.",
        ),
        (
            "02",
            "The council debates",
            "Different models attack each other's reasoning.",
        ),
        (
            "03",
            "Read the brief",
            "A recommendation, its tradeoffs, and next steps.",
        ),
    ];
    if ui.available_width() < 650.0 {
        for (number, title, detail) in steps {
            step_card(ui, app, number, title, detail);
        }
    } else {
        ui.columns(3, |columns| {
            for (column, (number, title, detail)) in columns.iter_mut().zip(steps) {
                step_card(column, app, number, title, detail);
            }
        });
    }
}

fn step_card(ui: &mut Ui, app: &App, number: &str, title: &str, detail: &str) {
    Frame::new()
        .fill(app.theme.card_bg)
        .corner_radius(10)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_min_height(100.0);
            // `columns` lays its children out justified, which spreads a wrapped
            // label's glyphs across the line. Left-align and wrap instead.
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(number).small().color(app.theme.accent));
                ui.label(RichText::new(title).strong());
                ui.add(egui::Label::new(
                    RichText::new(detail).color(app.theme.text_muted),
                ));
            });
        });
}

/// A failed new launch must not relabel the retained decision as interrupted.
fn launch_failure_card(ui: &mut Ui, app: &mut App) {
    let Some(failure) = &app.launch_failure else {
        return;
    };
    let mut dismiss = false;
    Frame::new().fill(app.theme.warning.gamma_multiply(0.09))
        .inner_margin(16).corner_radius(8).show(ui, |ui| {
            theme::heading(ui, "New run could not start");
            ui.label(if app.has_workspace() {
                "Previous visible work remains available. This error belongs to the attempted new run, not to the decision below. No retry happens automatically."
            } else {
                "The run did not open. Review the setup before trying again. No retry happens automatically."
            });
            ui.add(egui::Label::new(&failure.message).wrap());
            dismiss = ui.button("Dismiss launch error").clicked();
            theme::disclosure(ui, "Attempted decision question").show(ui, |ui| {
                ui.add(egui::Label::new(&failure.question).wrap());
            });
        });
    if dismiss {
        app.launch_failure = None;
        app.status_message = "Launch error dismissed. Existing work is unchanged.".into();
    }
    ui.add_space(16.0);
}

/// A saved decision that could not be read has no arguments to retry and no
/// stage to resume, so it gets its own notice with only actions that apply.
fn load_failed_card(ui: &mut Ui, app: &mut App) {
    let attention = app.load_failure_attention && !egui::Popup::is_any_open(ui.ctx());
    let action = if app.active_tab == Tab::Sessions {
        "Refresh saved decisions"
    } else {
        "See all decisions"
    };
    let card = Frame::new()
        .fill(app.theme.warning.gamma_multiply(0.09))
        .stroke(egui::Stroke::new(1.5, app.theme.warning.gamma_multiply(0.6)))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("That saved decision could not be read")
                    .size(19.0)
                    .strong()
                    .color(app.theme.warning),
            );
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Previous visible work was not replaced. Refresh the saved-decision list, then retry or choose another session.",
                    )
                    .color(app.theme.text_muted),
                )
                .wrap(),
            );
            ui.add_space(8.0);
            let recovery = ui.add_sized([ui.available_width().min(220.0), 44.0], egui::Button::new(action));
            if attention { recovery.request_focus(); }
            if recovery.clicked() {
                app.active_tab = Tab::Sessions;
                app.refresh_sessions();
            }
        });
    if attention {
        card.response
            .scroll_to_me_animation(Some(egui::Align::TOP), egui::style::ScrollAnimation::none());
        if !ui.ctx().will_discard() && !ui.ctx().input(egui::InputState::is_scrolling) {
            app.load_failure_attention = false;
        }
    }
    ui.add_space(16.0);
}

fn unfinished_card(ui: &mut Ui, app: &mut App) {
    Frame::new()
        .fill(app.theme.card_bg)
        .stroke(egui::Stroke::new(1.0, app.theme.border))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(if app.stopped_by_user { "Stopped by you — no new brief" } else { "Unfinished — no decision brief" })
                    .size(19.0)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Completed arguments remain readable. Continuing retains them; a new run repeats every stage. Review the execution plan before sending more requests.",
                    )
                    .color(app.theme.text_muted),
                )
                .wrap(),
            );
            ui.label(RichText::new(format!("{} completed stages confirmed in saved storage; other visible work may only be available here.", app.persisted_stages)).small().color(app.theme.text_muted));
            if app.last_config.is_none() {
                ui.label("Saved settings are unavailable. Read existing work or configure a new run; continuation is not available.");
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if app.last_config.is_some()
                    && ui
                        .button("Review remaining work")
                        .on_hover_text("Keeps the arguments already written and continues from there")
                        .clicked()
                {
                    app.retry_stage();
                }
                if ui
                    .button("Review a new run")
                    .on_hover_text("Runs every argument again")
                    .clicked()
                {
                    app.rerun_current();
                }
            });
            completed_work_actions(ui, app);
        });
    ui.add_space(16.0);
}

fn error_card(ui: &mut Ui, app: &mut App, error: &str) {
    Frame::new()
        .fill(app.theme.error.gamma_multiply(0.09))
        .stroke(egui::Stroke::new(1.5, app.theme.error.gamma_multiply(0.6)))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(if app.has_brief() { "Run interrupted — brief available here" } else { "Debate interrupted — no new decision brief" })
                    .size(19.0)
                    .strong()
                    .color(app.theme.error),
            );
            ui.add_space(4.0);
            ui.add(egui::Label::new(error).wrap());
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    if app.has_brief() { "The generated brief remains readable here. Copy it for safekeeping before leaving; only the storage confirmations below establish what was saved." } else { "Completed arguments remain available here. Review remaining work to retain them and retry unfinished work. Provider charges may apply." },
                )
                .color(app.theme.text_muted),
            );
            ui.label(RichText::new(format!("{} completed stages confirmed in saved storage; other visible work may only be available here.", app.persisted_stages)).small().color(app.theme.text_muted));
            if app.last_config.is_none() {
                ui.label("Saved settings are unavailable. Read existing work or configure a new run; continuation is not available.");
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if !app.has_brief() && app.last_config.is_some()
                    && ui
                        .button("Review remaining work")
                        .clicked()
                {
                    app.retry_stage();
                }
                if app.has_brief() && ui.button("Copy brief for safekeeping").clicked() {
                    let practice = app.last_config.as_ref().is_some_and(|c| c.dry_run);
                    ui.ctx().copy_text(crate::brief::qualified_export(&app.brief_text, practice));
                }
                if ui
                    .button("New run with different settings")
                    .clicked()
                {
                    app.composer_open = true;
                }
            });
            completed_work_actions(ui, app);
        });
    ui.add_space(16.0);
}

fn completed_work_actions(ui: &mut Ui, app: &mut App) {
    if app.has_brief()
        || !app
            .turns
            .iter()
            .any(|turn| turn.status == crate::model::TurnStatus::Complete)
    {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("Read completed arguments").clicked() {
            app.read_completed_arguments();
        }
        if ui.button("Copy completed arguments").clicked() {
            let practice = app.demo_mode
                || app
                    .last_config
                    .as_ref()
                    .is_some_and(|config| config.dry_run);
            ui.ctx().copy_text(crate::brief::completed_arguments_export(
                &app.current_question,
                &app.turns,
                practice,
            ));
        }
    });
}

fn footer(ui: &mut Ui, app: &App) {
    theme::region(ui, egui::accesskit::Role::ContentInfo, "Run status");
    ui.horizontal_wrapped(|ui| {
        theme::horizontal_group(ui, "Current run status");
        ui.add_space(12.0);
        if app.is_running {
            ui.spinner();
            ui.label(RichText::new("Working").color(app.theme.warning));
        } else if app.launch_failure.is_some() {
            ui.label(RichText::new("Not started").color(app.theme.warning));
        } else if app.load_failed() {
            ui.label(RichText::new("Not opened").color(app.theme.warning));
        } else if app.error.is_some() || app.stopped_by_user {
            ui.label(RichText::new("Stopped").color(app.theme.error));
        } else if app.needs_brief() {
            ui.label(RichText::new("Unfinished").color(app.theme.warning));
        } else if app.has_brief() {
            ui.label(RichText::new("Brief ready").color(app.theme.success));
        } else {
            ui.label(RichText::new("Ready").color(app.theme.success));
        }
        let status = ui.add(
            egui::Label::new(
                RichText::new(&app.status_message)
                    .small()
                    .color(app.theme.text_muted),
            )
            .wrap(),
        );
        ui.ctx().accesskit_node_builder(status.id, |node| {
            node.set_role(egui::accesskit::Role::Status);
            node.set_live(egui::accesskit::Live::Polite);
            node.set_label(&app.status_message);
        });
    });
}
