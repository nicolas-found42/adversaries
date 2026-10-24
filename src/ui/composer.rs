use crate::app::App;
use crate::roles::{self, PROVIDERS, ROLES, Selection};
use egui::{Context, DragValue, Id, RichText, TextEdit, Ui};
use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;

type AttachmentResult = Receiver<Result<String, String>>;

pub struct ComposerState {
    pub question: String,
    pub rounds: u32,
    pub dry_run: bool,
    pub roles: BTreeMap<String, Selection>,
    pub context_files: Vec<String>,
    pub(crate) searches: BTreeMap<String, String>,
    pub(crate) attachment: Option<AttachmentResult>,
    pub(crate) message: String,
}

impl Default for ComposerState {
    fn default() -> Self {
        Self {
            question: String::new(),
            rounds: 1,
            dry_run: false,
            roles: roles::defaults(),
            context_files: Vec::new(),
            searches: BTreeMap::new(),
            attachment: None,
            message: String::new(),
        }
    }
}

pub fn show_modal(ctx: &Context, app: &mut App) {
    poll_attachment(&mut app.composer_state);
    if app.catalog.is_empty() && app.catalog_message.starts_with("Load provider") {
        app.refresh_catalog();
    }
    let response = egui::Modal::new(Id::unique("decision_composer")).show(ctx, |ui| {
        ui.set_width(740.0_f32.min(ctx.content_rect().width() - 80.0));
        ui.heading("Frame your decision");
        ui.weak("A clear question. Your context. A council chosen by you.");
        ui.add_space(8.0);
        let height = (ctx.content_rect().height() - 240.0).max(180.0);
        egui::ScrollArea::vertical()
            .id_salt("composer_content")
            .max_height(height)
            .show(ui, |ui| {
                question_editor(ui, &mut app.composer_state);
                ui.add_space(8.0);
                council_editor(ui, app);
            });
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            let state = &app.composer_state;
            let enabled = !state.question.trim().is_empty()
                && state.attachment.is_none()
                && state.roles.values().all(|r| !r.model.trim().is_empty());
            let calls = 2 + state.rounds * 4;
            ui.weak(if state.dry_run {
                "Offline simulation · no API calls".into()
            } else {
                format!("{calls} calls · one attempt per stage")
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(if app.is_running {
                            "Stop & start new decision"
                        } else {
                            "Start decision"
                        }),
                    )
                    .clicked()
                {
                    app.submit_composer();
                }
                if ui.button("Cancel").clicked() {
                    app.composer_open = false;
                }
            });
        });
    });
    if response.should_close() {
        app.composer_open = false;
    }
}

fn question_editor(ui: &mut Ui, state: &mut ComposerState) {
    ui.strong("What are you deciding?");
    ui.add(TextEdit::multiline(&mut state.question).desired_rows(3).desired_width(f32::INFINITY)
        .hint_text("Should we build a mobile app or start with responsive web?\nInclude your deadline, options, and constraints."));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                state.attachment.is_none(),
                egui::Button::new("+ Attach context"),
            )
            .clicked()
        {
            state.attachment = Some(crate::attachments::choose());
        }
        ui.weak("Documents, notes, or numbers for the council");
    });
    let mut remove = None;
    for (idx, file) in state.context_files.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(file.rsplit('/').next().unwrap_or(file));
            if ui.small_button("Remove").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some(idx) = remove {
        state.context_files.remove(idx);
    }
    if !state.message.is_empty() {
        ui.weak(&state.message);
    }
}

fn poll_attachment(state: &mut ComposerState) {
    let Some(rx) = &state.attachment else {
        return;
    };
    if let Ok(result) = rx.try_recv() {
        match result {
            Ok(path) => {
                state.context_files.push(path);
                state.message.clear();
            }
            Err(message) => state.message = message,
        }
        state.attachment = None;
    }
}

fn council_editor(ui: &mut Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.strong("Your council");
        if ui
            .add_enabled(!app.catalog_loading(), egui::Button::new("Refresh models"))
            .clicked()
        {
            app.refresh_catalog();
        }
        if app.catalog_loading() {
            ui.spinner();
        }
    });
    ui.label(
        RichText::new(&app.catalog_message)
            .small()
            .color(app.theme.text_muted),
    );
    let state = &mut app.composer_state;
    for (role, title) in ROLES {
        let Some(selection) = state.roles.get_mut(role) else {
            continue;
        };
        ui.push_id(role, |ui| {
            egui::Frame::new()
                .fill(app.theme.bg)
                .corner_radius(7)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.add_sized(
                            [128.0, 28.0],
                            egui::Label::new(RichText::new(title).strong()),
                        );
                        provider_select(ui, selection);
                        let catalog = app
                            .catalog
                            .iter()
                            .find(|c| c.provider == selection.provider);
                        model_select(
                            ui,
                            selection,
                            catalog,
                            state.searches.entry(role.into()).or_default(),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(138.0);
                        ui.label(
                            RichText::new("Model ID")
                                .small()
                                .color(app.theme.text_muted),
                        );
                        ui.add(
                            TextEdit::singleline(&mut selection.model)
                                .desired_width(390.0)
                                .font(egui::TextStyle::Small),
                        );
                    });
                    if let Some(catalog) = app
                        .catalog
                        .iter()
                        .find(|c| c.provider == selection.provider)
                    {
                        if !catalog.error.is_empty() {
                            ui.label(
                                RichText::new(&catalog.error)
                                    .small()
                                    .color(app.theme.warning),
                            );
                        }
                    }
                });
        });
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Debate rounds");
        ui.add(DragValue::new(&mut state.rounds).range(1..=5));
        ui.checkbox(&mut state.dry_run, "Dry run · no API cost");
    });
    ui.weak("Each round runs three critiques and one rebuttal. Any provider and model can fill any role.");
}

fn provider_select(ui: &mut Ui, selection: &mut Selection) {
    egui::ComboBox::from_id_salt("provider")
        .width(140.0)
        .selected_text(&selection.provider)
        .show_ui(ui, |ui| {
            for provider in PROVIDERS {
                if ui
                    .selectable_value(&mut selection.provider, provider.into(), provider)
                    .changed()
                {
                    selection.model = roles::default_model(provider).into();
                }
            }
        });
}

fn model_select(
    ui: &mut Ui,
    selection: &mut Selection,
    catalog: Option<&roles::ProviderCatalog>,
    search: &mut String,
) {
    egui::ComboBox::from_id_salt("model")
        .width(300.0)
        .selected_text(&selection.model)
        .show_ui(ui, |ui| {
            ui.add(TextEdit::singleline(search).hint_text("Search models…"));
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    let models = catalog.map_or_else(
                        || vec![roles::default_model(&selection.provider).into()],
                        |c| c.models.clone(),
                    );
                    for model in models
                        .iter()
                        .filter(|m| m.to_lowercase().contains(&search.to_lowercase()))
                    {
                        ui.selectable_value(&mut selection.model, model.clone(), model);
                    }
                });
            ui.weak("You can also enter an exact model ID below.");
        });
}
