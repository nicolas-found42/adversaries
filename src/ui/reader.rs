use egui::{RichText, ScrollArea, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

pub fn document(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    cache: &mut CommonMarkCache,
) {
    ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink(false)
        .show(ui, |ui| {
            let padding = ((ui.available_width() - 820.0) / 2.0).clamp(0.0, 120.0);
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: padding as i8,
                    right: padding as i8,
                    top: 8,
                    bottom: 32,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().min(820.0));
                    ui.spacing_mut().item_spacing.y = 12.0;
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    CommonMarkViewer::new().show(ui, cache, text);
                });
        });
}

pub fn placeholder(ui: &mut Ui, title: &str, detail: &str) {
    ui.add_space(32.0);
    ui.label(RichText::new(title).size(22.0).strong());
    ui.add_space(8.0);
    ui.weak(detail);
}
