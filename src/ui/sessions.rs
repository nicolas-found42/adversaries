use egui::{RichText, Ui};

use crate::app::App;

pub fn render_sessions_view(ui: &mut Ui, app: &mut App) {
    crate::ui::theme::page_heading(ui, RichText::new("All decisions").color(app.theme.accent));
    history_status(ui, app);
    if app.sessions.is_empty() {
        ui.label(
            RichText::new(if app.history_error.is_some() {
                "The saved-decision list is unavailable. Refresh to retry."
            } else if app.history_loading() {
                "Waiting for saved decisions…"
            } else {
                "No saved decisions yet. Start a decision to see it here."
            })
            .color(app.theme.text_muted),
        );
        return;
    }
    ui.label("Open saved work without running models. Continue or repeat only after reviewing an execution plan.");
    ui.add(
        egui::TextEdit::singleline(&mut app.history_search)
            .id_salt("history_search")
            .hint_text("Find a saved decision"),
    );
    let visible = app.visible_sessions();
    let Some(first) = visible.first() else {
        ui.label("No saved decisions match this search.");
        return;
    };
    if !visible.contains(&app.selected_session) {
        app.selected_session = *first;
    }
    if ui.available_width() < 600.0 {
        let path = app.sessions[app.selected_session].path_str.clone();
        open_saved_button(ui, app, &path, "Open selected decision");
        render_session_list(ui, app);
        ui.separator();
        crate::ui::theme::heading(ui, "Selected decision details");
        render_session_preview(ui, app, false);
    } else {
        ui.columns(2, |columns| {
            left_aligned(&mut columns[0], |ui| render_session_list(ui, app));
            left_aligned(&mut columns[1], |ui| render_session_preview(ui, app, true));
        });
    }
}

fn history_status(ui: &mut Ui, app: &mut App) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !app.history_loading(),
                egui::Button::new("Refresh decisions"),
            )
            .clicked()
        {
            app.refresh_sessions();
        }
        if app.history_loading() {
            ui.spinner();
            ui.label("Loading saved decisions…");
        }
    });
    pending_open(ui, app);
    if let Some(error) = &app.history_error {
        ui.label(RichText::new(error).color(app.theme.error));
    }
    if app.load_failed() {
        super::load_failed_card(ui, app);
    }
    if app.is_running {
        ui.label("A debate is active. Reading this list does not stop it. Return to the active decision before opening another saved session.");
        if ui.button("Return to active decision").clicked() {
            app.active_tab = crate::model::Tab::Debate;
        }
    }
}

fn pending_open(ui: &mut Ui, app: &mut App) {
    let Some(question) = app.opening_question() else {
        return;
    };
    let title = crate::ui::short_question(question);
    ui.add(egui::Label::new(format!("Opening: {title}")).wrap());
    ui.label("Your current work remains available. Opening saved work does not run models.");
    let cancel = ui.button("Cancel opening").clicked();
    if title != question {
        crate::ui::theme::disclosure(ui, "Read full opening question").show(ui, |ui| {
            ui.label(question);
        });
    }
    if cancel {
        app.cancel_session_open();
    }
}

/// `columns` justifies its children, which stretches a wrapped label's glyphs
/// across the line. Render inside a left-aligned scope instead.
fn left_aligned(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        ui.set_width(ui.available_width());
        contents(ui);
    });
}

fn render_session_list(ui: &mut Ui, app: &mut App) {
    ui.push_id("session_list", |ui| {
        for idx in 0..app.sessions.len() {
            session_row(ui, app, idx);
        }
        ui.add_space(24.0);
    });
}

fn session_row(ui: &mut Ui, app: &mut App, idx: usize) {
    let session = app.sessions[idx].clone();
    if !session
        .question
        .to_lowercase()
        .contains(&app.history_search.to_lowercase())
    {
        return;
    }
    let path = session.path_str.clone();
    let title = crate::ui::short_question(&session.question);
    let mode = match session.dry_run {
        Some(true) => "Practice",
        Some(false) => "Real provider run",
        None => "Mode not recorded",
    };
    let meta = format!("{} · {mode}", session_meta(&session));
    let selected = idx == app.selected_session;
    let row = egui::Frame::new()
        .fill(if selected {
            app.theme.card_bg
        } else {
            app.theme.bg
        })
        .stroke(egui::Stroke::new(
            2.0,
            if selected {
                app.theme.border_focus
            } else {
                app.theme.border
            },
        ))
        .corner_radius(8)
        .inner_margin(12);
    let painted = row
        .show(ui, |ui| {
            // The named interactive row below represents these painted labels.
            ui.ctx()
                .accesskit_node_builder(ui.unique_id(), egui::accesskit::Node::set_hidden);
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(&title).strong()).wrap());
            ui.add(egui::Label::new(
                RichText::new(&meta).small().color(app.theme.text_muted),
            ));
        })
        .response;
    let response = ui.interact(
        painted.rect,
        ui.make_persistent_id(("saved_decision", &path)),
        egui::Sense::click(),
    );
    // The frame is a clickable region on its own, so it needs a name to
    // appear in the accessibility tree at all.
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            true,
            selected,
            format!("{title} · {meta}"),
        )
    });
    select_session_row(&response, app, idx, &path);
    ui.add_space(8.0);
}

fn select_session_row(response: &egui::Response, app: &mut App, idx: usize, path: &str) {
    if app.selection_scroll == Some(crate::model::Tab::Sessions) && idx == app.selected_session {
        response.request_focus();
        response.scroll_to_me_animation(
            Some(egui::Align::Center),
            egui::style::ScrollAnimation::none(),
        );
    }
    if response.clicked() {
        app.selected_session = idx;
    }
    if response.double_clicked()
        || (response.clicked() && !response.clicked_by(egui::PointerButton::Primary))
    {
        app.load_session_by_path(path);
    }
}

fn render_session_preview(ui: &mut Ui, app: &mut App, show_open: bool) {
    // Copy what the pane needs so the action below can mutate `app` without
    // holding a borrow of the session list.
    let Some(session) = app.sessions.get(app.selected_session) else {
        return;
    };
    let question = session.question.clone();
    let path = session.path_str.clone();
    let file_name = session.file_name.clone();
    let meta = format!(
        "{} · {} {} · {}",
        friendly_date(&session.created_at),
        session.rounds,
        if session.rounds == 1 {
            "round"
        } else {
            "rounds"
        },
        session_state(session)
    );
    let empty = session.turn_count == 0 && !session.has_brief;
    ui.label(RichText::new(&question).strong().color(app.theme.text));
    ui.add_space(4.0);
    ui.label(RichText::new(meta).color(app.theme.text_muted));
    if empty {
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "No completed arguments were saved. Open this decision to inspect any partial output and recovery options.",
            )
            .color(app.theme.warning),
        );
    }
    if let Some(interruption) = &session.interruption {
        ui.add_space(8.0);
        ui.label(
            RichText::new("Recorded interruption")
                .strong()
                .color(app.theme.warning),
        );
        ui.label(format!(
            "Stage {} · {} · {}",
            interruption.stage_index, interruption.role, interruption.model
        ));
        ui.label(interruption.recorded_reason());
        ui.weak("Opening saved work does not retry this stage or send new model requests.");
    }
    if show_open {
        ui.add_space(12.0);
        open_saved_button(ui, app, &path, "Open this decision");
    }
    ui.add_space(8.0);
    crate::ui::theme::disclosure(ui, "Storage details").show(ui, |ui| {
        ui.label(format!("Saved as {file_name}"));
    });
}

fn open_saved_button(ui: &mut Ui, app: &mut App, path: &str, label: &str) {
    if ui
        .add_enabled(
            !app.is_running && !app.session_opening(),
            egui::Button::new(label).min_size(egui::vec2(ui.available_width().min(320.0), 44.0)),
        )
        .clicked()
    {
        app.load_session_by_path(path);
    }
}

fn session_meta(session: &crate::model::SessionSummary) -> String {
    format!(
        "{} · {} {} · {}",
        friendly_date(&session.created_at),
        session.turn_count,
        if session.turn_count == 1 {
            "stage"
        } else {
            "stages"
        },
        session_state(session)
    )
}

fn session_state(session: &crate::model::SessionSummary) -> &'static str {
    match (
        session.has_brief,
        session.interruption.is_some(),
        session.turn_count,
    ) {
        (true, true, _) => "Brief ready · interruption recorded",
        (true, false, _) => "Brief ready",
        (false, true, _) => "Interrupted · no brief",
        (false, false, 0) => "Unfinished · no completed arguments",
        _ => "Unfinished · no brief",
    }
}

/// Turns a stored timestamp into a date a reader does not have to decode.
/// Accepts the `YYYY-MM-DD…` prefix used by both report and sidecar names and
/// falls back to whatever was stored.
pub(crate) fn friendly_date(created_at: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date: Vec<&str> = created_at
        .get(..10)
        .unwrap_or_default()
        .split('-')
        .collect();
    let [year, month, day] = date.as_slice() else {
        return created_at.to_string();
    };
    let (Ok(month), Ok(day)) = (
        month.parse::<usize>(),
        day.trim_start_matches('0').parse::<u8>(),
    ) else {
        return created_at.to_string();
    };
    let date = MONTHS.get(month.wrapping_sub(1)).map_or_else(
        || created_at.to_string(),
        |name| format!("{day} {name} {year}"),
    );
    // Two runs of the same question look identical by date alone. The clock
    // time is what lets a reader tell them apart. Reports without a JSON
    // sidecar carry `YYYY-MM-DD_HHMM`, so both shapes are read.
    if let Some(time) = created_at.get(11..16) {
        if time.as_bytes().get(2) == Some(&b':') {
            return format!("{date}, {time}");
        }
    }
    match created_at.get(11..15) {
        Some(hhmm) if hhmm.len() == 4 && hhmm.bytes().all(|b| b.is_ascii_digit()) => {
            format!("{date}, {}:{}", &hhmm[..2], &hhmm[2..])
        }
        _ => date,
    }
}

#[cfg(test)]
mod tests {
    use super::friendly_date;

    #[test]
    fn keeps_the_clock_time_when_the_stamp_has_one() {
        // Two runs of one question must stay distinguishable by name alone.
        assert_eq!(
            friendly_date("2026-10-23T19:10:49.123"),
            "23 Oct 2026, 19:10"
        );
        assert_eq!(
            friendly_date("2026-10-23T19:18:23.000"),
            "23 Oct 2026, 19:18"
        );
    }

    #[test]
    fn reads_the_time_from_a_stem_without_a_sidecar() {
        // Markdown-only reports store `YYYY-MM-DD_HHMM`; dropping the time there
        // leaves same-day runs indistinguishable.
        assert_eq!(
            friendly_date("2026-10-23_0123"),
            "23 Oct 2026, 01:23",
            "an old report still shows when it ran"
        );
    }

    #[test]
    fn falls_back_to_the_date_alone_or_the_raw_stamp() {
        assert_eq!(friendly_date("2026-10-23"), "23 Oct 2026");
        assert_eq!(friendly_date("Unknown"), "Unknown");
    }
}
