use egui::{RichText, ScrollArea, Ui};

use crate::app::App;

pub fn render_sessions_view(ui: &mut Ui, app: &mut App) {
    ui.heading(RichText::new("Session History").color(app.theme.accent));
    if app.sessions.is_empty() {
        ui.label(RichText::new("No saved sessions found in reports/.").color(app.theme.text_muted));
        return;
    }
    ui.columns(2, |columns| {
        render_session_list(&mut columns[0], app);
        render_session_preview(&mut columns[1], app);
    });
}

fn render_session_list(ui: &mut Ui, app: &mut App) {
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let count = app.sessions.len();
        for idx in 0..count {
            let session = &app.sessions[idx];
            let path = session.path_str.clone();
            let title = truncate(&session.question, 70);
            let meta = session_meta(session);
            let selected = idx == app.selected_session;
            let title_color = if selected {
                app.theme.accent
            } else {
                app.theme.text
            };
            let meta_color = app.theme.text_muted;
            ui.add_space(2.0);
            let label = ui.selectable_label(selected, RichText::new(title).color(title_color));
            ui.label(RichText::new(meta).small().color(meta_color));
            if label.clicked() {
                app.selected_session = idx;
            }
            if label.double_clicked() {
                app.load_session_by_path(&path);
            }
        }
    });
}

fn render_session_preview(ui: &mut Ui, app: &mut App) {
    let Some(session) = app.sessions.get(app.selected_session) else {
        return;
    };
    ui.label(
        RichText::new(&session.question)
            .strong()
            .color(app.theme.text),
    );
    ui.label(
        RichText::new(format!("file: {}", session.file_name))
            .small()
            .color(app.theme.text_muted),
    );
    ui.label(
        RichText::new(format!(
            "{} · {} rounds · {} turns · brief: {}",
            session.created_at,
            session.rounds,
            session.turn_count,
            if session.has_brief { "yes" } else { "no" }
        ))
        .color(app.theme.text_muted),
    );
    ui.separator();
    ui.label(
        RichText::new("Press Enter (or double-click) to replay this session in the debate viewer.")
            .color(app.theme.text_muted),
    );
}

fn session_meta(session: &crate::model::SessionSummary) -> String {
    format!(
        "{} · {} turns{}",
        session.created_at,
        session.turn_count,
        if session.has_brief {
            " · brief ✓"
        } else {
            ""
        }
    )
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}
