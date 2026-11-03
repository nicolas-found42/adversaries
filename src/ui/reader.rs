pub(crate) mod document;

use egui::{RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

pub fn document(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    cache: &mut CommonMarkCache,
) {
    ui.push_id(id, |ui| {
        let padding = ((ui.available_width() - 820.0) / 2.0).clamp(0.0, 120.0);
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: padding as i8,
                right: padding as i8,
                top: 8,
                bottom: 32,
            })
            .show(ui, |ui| {
                body(ui, text, cache);
            });
    });
}

pub fn body(ui: &mut Ui, text: &str, cache: &mut CommonMarkCache) {
    let scope = ui.next_auto_id().with("reader_document");
    ui.scope_builder(egui::UiBuilder::new().scope_id(scope), |ui| {
        visual_body(ui, text, cache);
        document::register(ui.ctx(), scope, text);
    });
}

fn visual_body(ui: &mut Ui, text: &str, cache: &mut CommonMarkCache) {
    ui.set_width(ui.available_width().min(820.0));
    ui.spacing_mut().item_spacing.y = 12.0;
    // Do not force a wrap mode here. `Ui::wrap_mode` consults `style.wrap_mode`
    // before the grid layout, so an override leaks into every markdown table
    // cell, which then wraps at the previous frame's column width and never
    // grows. A vertical `Ui` already resolves to `Wrap` for prose.
    let mut cursor = 0;
    for (start, end) in super::table::ranges(text) {
        markdown(ui, &text[cursor..start], cache);
        match super::table::parse(&text[start..end]) {
            Some(table) => super::table::ui(ui, &table),
            // Unparseable span: hand the source to the crate rather than drop it.
            None => markdown(ui, &text[start..end], cache),
        }
        cursor = end;
    }
    markdown(ui, &text[cursor..], cache);
}

fn markdown(ui: &mut Ui, text: &str, cache: &mut CommonMarkCache) {
    if text.trim().is_empty() {
        return;
    }
    CommonMarkViewer::new().show(ui, cache, text);
}

pub fn placeholder(ui: &mut Ui, title: &str, detail: &str) {
    ui.add_space(32.0);
    ui.add(egui::Label::new(RichText::new(title).size(22.0).strong()).wrap());
    ui.add_space(8.0);
    ui.add(egui::Label::new(RichText::new(detail).color(ui.visuals().weak_text_color())).wrap());
}
