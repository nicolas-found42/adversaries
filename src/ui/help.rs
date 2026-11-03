use egui::{Grid, RichText, Ui};

use crate::ui::theme::Theme;

pub fn render_help_view(ui: &mut Ui, theme: &Theme) {
    crate::ui::theme::page_intro(
        ui,
        theme,
        "MEET THE CREW",
        "How this works",
        "One question. Different points of view. You stay in charge.",
    );
    crate::ui::theme::card(theme).show(ui, |ui| {
    ui.push_id("help_content", |ui| {
        section(ui, theme, "What happens when you start");
        body(
            ui,
            theme,
            "Write a question and check the plan before starting. One AI model suggests an idea. Three others take turns looking for problems in the reasoning. The first model answers those challenges. Then a model writes a brief: a short guide to the options, risks, and next steps.",
        );
        ui.add_space(12.0);

        section(ui, theme, "Reading the result");
        body(
            ui,
            theme,
            "Read Decision brief first, then inspect Arguments or Claims & sources. Decision actions contains Review a new run and Run details. Source relationships and parsed debate outcomes describe the supplied text and model debate, not independent verification. A model-reported confidence figure is not a calibrated probability.",
        );
        ui.add_space(12.0);

        section(ui, theme, "Keyboard");
        responsive_rows(ui, "help_keys", |ui| {
            key_row(ui, theme, "Tab", "Move to the next control");
            key_row(ui, theme, "Shift+Tab", "Move to the previous control");
            key_row(ui, theme, "Enter or Space", "Press the control you are on");
            key_row(ui, theme, "Esc", "Close a menu or dialog, or return from source inspection");
            key_row(ui, theme, "1 to 6", "Jump to a view");
            key_row(ui, theme, "Arrow keys or j / k", "Move through arguments, claims, or saved decisions");
            key_row(ui, theme, "n", "Start a new decision");
            key_row(ui, theme, "c", "Stop the run in progress");
            key_row(ui, theme, "r", "Review a new run for the same question");
            #[cfg(not(target_arch = "wasm32"))]
            key_row(ui, theme, "q", "Close the app");
            ui.end_row();
        });
        body(
            ui,
            theme,
            "Keys 1 to 6 are: 1 Arguments, 2 Decision brief, 3 Claims & sources, 4 Decisions, 5 Help, 6 Run details. Shortcuts yield to text editing, dialogs and open menus. Tab follows the available controls.",
        );
        ui.add_space(12.0);

        section(ui, theme, "Before a real run");
        body(ui, theme, "Review run shows the role assignments, recipients, included document text and remaining stages. Practice example uses simulated responses. Real provider run may incur charges; credentials being configured does not guarantee model availability. Nothing starts until you confirm the reviewed plan.");
        ui.add_space(12.0);
        section(ui, theme, "If work stops or cannot be opened");
        body(ui, theme, "Read completed arguments preserves access without starting inference. Review remaining work retains eligible completed stages and shows what must run next. New run with different settings starts a separately reviewed run. Stop run does not guarantee a refund for requests already sent. Confirmed saved work is shown separately from output available only in this window.");
        body(ui, theme, "Opening saved work does not run models. If a saved decision cannot be read, your open work remains available. Refresh decisions to retry. Some older records lack the settings required for continuation; their arguments can still be read.");
        ui.add_space(12.0);
        section(ui, theme, "The order of the debate");
        responsive_rows(ui, "help_protocol", |ui| {
            protocol_row(ui, "Opening", theme.mistral, "States a position and numbers its claims C1, C2 and so on");
            protocol_row(ui, "Critique 1", theme.openrouter, "Attacks those claims");
            protocol_row(ui, "Critique 2", theme.ollama, "Attacks the first critique");
            protocol_row(ui, "Critique 3", theme.opencode, "Attacks the second critique");
            protocol_row(ui, "Rebuttal", theme.mistral, "Concedes or defends each attack and revises the recommendation");
            protocol_row(ui, "Brief", theme.mistral, "Writes the decision brief");
            ui.end_row();
        });
        ui.add_space(12.0);
        section(ui, theme, "Where things are kept");
        body(
            ui,
            theme,
            "Desktop decisions are stored in the configured reports folder. Browser decisions and uploaded documents are stored by the Council server. Real runs send the question and included document content to the selected providers.",
        );
    });
    });
}

fn section(ui: &mut Ui, theme: &Theme, title: &str) {
    crate::ui::theme::heading(
        ui,
        RichText::new(title).size(19.0).strong().color(theme.accent),
    );
    ui.add_space(4.0);
}

fn body(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(theme.text_muted)).wrap());
}

fn key_row(ui: &mut Ui, theme: &Theme, keys: &str, description: &str) {
    ui.label(RichText::new(keys).strong().color(theme.highlight));
    ui.add(egui::Label::new(description).wrap());
    ui.end_row();
}

fn protocol_row(ui: &mut Ui, label: &str, color: egui::Color32, description: &str) {
    ui.label(RichText::new(label).strong().color(color));
    ui.add(egui::Label::new(description).wrap());
    ui.end_row();
}

/// Compact help keeps the key/role above its explanation instead of giving a
/// grid's longest first-column label permission to widen the whole document.
fn responsive_rows(ui: &mut Ui, id: &str, contents: impl FnOnce(&mut Ui)) {
    if ui.available_width() < 500.0 {
        contents(ui);
    } else {
        Grid::new(id).num_columns(2).show(ui, contents);
    }
}
