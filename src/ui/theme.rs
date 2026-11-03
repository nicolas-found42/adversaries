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
    /// Animation and smooth-scroll duration. Zero when the platform reports a
    /// reduced-motion preference.
    pub motion_seconds: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color32::from_rgb(18, 17, 36),
            card_bg: Color32::from_rgb(31, 29, 55),
            // 3.8:1 against `bg`, so control outlines clear WCAG 1.4.11.
            border: Color32::from_rgb(112, 109, 146),
            border_focus: Color32::from_rgb(185, 160, 255),
            text: Color32::from_rgb(247, 244, 255),
            text_muted: Color32::from_rgb(193, 190, 215),
            accent: Color32::from_rgb(190, 166, 255),
            highlight: Color32::from_rgb(255, 213, 112),
            success: Color32::from_rgb(104, 230, 188),
            warning: Color32::from_rgb(255, 213, 112),
            error: Color32::from_rgb(255, 148, 170),
            mistral: Color32::from_rgb(255, 175, 134),
            openrouter: Color32::from_rgb(190, 166, 255),
            ollama: Color32::from_rgb(245, 156, 230),
            opencode: Color32::from_rgb(104, 230, 188),
            motion_seconds: 0.2,
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
        visuals.extreme_bg_color = Color32::from_rgb(14, 13, 30);
        visuals.faint_bg_color = self.card_bg;
        visuals.override_text_color = Some(self.text);
        visuals.selection.bg_fill = self.border_focus.gamma_multiply(0.22);
        visuals.selection.stroke = Stroke::new(1.0, self.accent);
        visuals.widgets.noninteractive.fg_stroke.color = self.text_muted;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.inactive.weak_bg_fill = self.card_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(56, 47, 88);
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(65, 52, 98);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, self.accent);
        visuals.window_corner_radius = egui::CornerRadius::same(18);
        visuals.window_stroke = Stroke::new(1.0, self.border);
        visuals.hyperlink_color = self.accent;
        ctx.set_visuals(visuals);
        ctx.style_mut_of(egui::Theme::Dark, |style| {
            style.animation_time = self.motion_seconds;
            style.scroll_animation = egui::style::ScrollAnimation {
                duration: egui::Rangef::new(0.0, self.motion_seconds),
                ..Default::default()
            };
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(16.0, 10.0);
            style.spacing.interact_size.y = 44.0;
            // A scrollbar is the only cue that more content exists below.
            style.spacing.scroll.floating = false;
            style.spacing.scroll.bar_width = 10.0;
            style.spacing.scroll.floating_width = 10.0;
            style.spacing.scroll.dormant_background_opacity = 0.08;
            style.spacing.scroll.dormant_handle_opacity = 0.35;
            style.spacing.scroll.active_handle_opacity = 0.6;
            // A disabled control has to look unavailable without becoming
            // unreadable. 0.45 keeps its label above 3:1 on the background.
            style.visuals.disabled_alpha = 0.45;
            // egui styles a focused widget with `active`, so give it a ring a
            // keyboard user can actually see.
            style.visuals.widgets.active.bg_stroke =
                Stroke::new(2.0, Color32::from_rgb(190, 166, 255));
            // A scroll bar paints its handle from the inactive widget fill by
            // default, which the dark theme leaves near-white. `foreground_color`
            // switches it to the text tone instead; overriding `bg_fill` would
            // also fill every checkbox box solid, including "Dry run".
            style.spacing.scroll.foreground_color = true;
            style
                .text_styles
                .insert(TextStyle::Body, FontId::new(17.0, FontFamily::Proportional));
            style.text_styles.insert(
                TextStyle::Button,
                FontId::new(16.0, FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Small,
                FontId::new(14.0, FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Heading,
                FontId::new(24.0, FontFamily::Proportional),
            );
        });
    }
}

/// Landmarks and flow groups describe the shared interface, independent of its
/// visual layout. Browser controls consume these same semantics.
pub(crate) fn region(ui: &egui::Ui, role: egui::accesskit::Role, label: &str) {
    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
        node.set_role(role);
        node.set_label(label);
    });
}

pub(crate) fn horizontal_group(ui: &egui::Ui, label: &str) {
    region(ui, egui::accesskit::Role::Group, label);
    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
        node.set_orientation(egui::accesskit::Orientation::Horizontal);
    });
}

/// Typography alone does not give egui labels a heading role.
pub fn heading(ui: &mut egui::Ui, text: impl Into<egui::RichText>) -> egui::Response {
    let text = text.into();
    let label = text.text().to_owned();
    let response = ui.heading(text);
    mark_heading(ui, &response, &label, 2);
    response
}

pub(crate) fn page_heading(ui: &mut egui::Ui, text: impl Into<egui::RichText>) {
    let response = heading(ui, text);
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_level(1));
}

pub(crate) fn mark_heading(ui: &egui::Ui, response: &egui::Response, label: &str, level: usize) {
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Heading);
        node.set_level(level);
        node.set_label(label);
    });
}

/// egui's default disclosure title extends onto one line. Pre-wrap its galley
/// so long labels cannot widen the shared reader or a compact review dialog.
pub fn disclosure(ui: &egui::Ui, title: impl Into<egui::WidgetText>) -> egui::CollapsingHeader {
    let width =
        (ui.available_width() - ui.spacing().indent - ui.spacing().button_padding.x).max(1.0);
    let galley =
        title
            .into()
            .into_galley(ui, Some(egui::TextWrapMode::Wrap), width, TextStyle::Button);
    egui::CollapsingHeader::new(galley)
}

/// Shared surfaces keep every native and browser view in the idea-crew family.
pub fn card(theme: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.card_bg)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(16)
        .inner_margin(16)
}

pub fn feature_card(theme: &Theme, color: Color32) -> egui::Frame {
    card(theme)
        .stroke(Stroke::new(1.5, color))
        .fill(theme.card_bg)
}

pub fn primary_button(theme: &Theme, text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.into()).strong().color(theme.bg))
        .fill(theme.accent)
        .stroke(Stroke::NONE)
        .corner_radius(12)
        .min_size(egui::vec2(0.0, 44.0))
        .wrap()
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(text).small().strong().color(color)).wrap(),
            );
        });
}

pub fn page_intro(ui: &mut egui::Ui, theme: &Theme, kicker: &str, title: &str, detail: &str) {
    badge(ui, kicker, theme.accent);
    page_heading(ui, title);
    ui.add(egui::Label::new(egui::RichText::new(detail).color(theme.text_muted)).wrap());
    ui.add_space(8.0);
}

#[cfg(test)]
mod tests {
    use super::Theme;

    #[test]
    fn motion_setting_reaches_egui_style() {
        // `prefers_reduced_motion` sets this field, so the field must be what
        // egui animates with; asserting on the style proves the seam is wired.
        let ctx = egui::Context::default();
        Theme {
            motion_seconds: 0.0,
            ..Theme::default()
        }
        .apply(&ctx);
        let still = ctx.style_of(egui::Theme::Dark).animation_time;
        Theme::default().apply(&ctx);
        let animated = ctx.style_of(egui::Theme::Dark).animation_time;
        assert_eq!(still, 0.0, "reduced motion must stop animation");
        assert_eq!(animated, 0.2, "default motion must animate");
    }
}
