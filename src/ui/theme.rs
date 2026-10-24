use egui::{Color32, Context, FontFamily, FontId, Stroke, TextStyle, Visuals};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color32,
    pub card_bg: Color32,
    pub border: Color32,
    pub border_focus: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub highlight: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub mistral: Color32,
    pub openrouter: Color32,
    pub ollama: Color32,
    pub opencode: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color32::from_rgb(24, 25, 28),
            card_bg: Color32::from_rgb(32, 34, 38),
            border: Color32::from_rgb(51, 54, 61),
            border_focus: Color32::from_rgb(129, 161, 238),
            text: Color32::from_rgb(232, 231, 228),
            text_muted: Color32::from_rgb(159, 162, 170),
            accent: Color32::from_rgb(156, 182, 247),
            highlight: Color32::from_rgb(224, 191, 130),
            success: Color32::from_rgb(142, 193, 167),
            warning: Color32::from_rgb(224, 191, 130),
            error: Color32::from_rgb(237, 150, 151),
            mistral: Color32::from_rgb(227, 164, 124),
            openrouter: Color32::from_rgb(156, 182, 247),
            ollama: Color32::from_rgb(187, 166, 218),
            opencode: Color32::from_rgb(142, 193, 167),
        }
    }
}

impl Theme {
    pub fn model_color(&self, model: &str) -> Color32 {
        let lower = model.to_lowercase();
        if lower.contains("mistral") {
            self.mistral
        } else if lower.contains("openrouter") {
            self.openrouter
        } else if lower.contains("ollama") {
            self.ollama
        } else if lower.contains("opencode") {
            self.opencode
        } else {
            self.accent
        }
    }

    pub fn apply(&self, ctx: &Context) {
        ctx.set_theme(egui::Theme::Dark);
        let mut visuals = Visuals::dark();
        visuals.panel_fill = self.bg;
        visuals.window_fill = self.card_bg;
        visuals.extreme_bg_color = Color32::from_rgb(20, 21, 24);
        visuals.faint_bg_color = self.card_bg;
        visuals.override_text_color = Some(self.text);
        visuals.selection.bg_fill = self.border_focus.gamma_multiply(0.22);
        visuals.selection.stroke = Stroke::new(1.0, self.accent);
        visuals.widgets.noninteractive.fg_stroke.color = self.text_muted;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.inactive.weak_bg_fill = self.card_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(49, 54, 65);
        ctx.set_visuals(visuals);
        ctx.style_mut_of(egui::Theme::Dark, |style| {
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(13.0, 8.0);
            style.spacing.interact_size.y = 32.0;
            style
                .text_styles
                .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
            style.text_styles.insert(
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Heading,
                FontId::new(24.0, FontFamily::Proportional),
            );
        });
    }
}
