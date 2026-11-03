use crate::app::App;
use crate::roles::{self, PROVIDERS, ROLES, Selection};
use egui::{Context, DragValue, Id, RichText, TextEdit, Ui};
use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, TryRecvError};

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
            dry_run: true,
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
        ui.set_width((ctx.content_rect().width() - 40.0).clamp(80.0, 740.0));
        let compact = ui.available_width() < 500.0 || ctx.content_rect().height() < 650.0;
        if compact {
            egui::ScrollArea::vertical()
                .id_salt("compact_composer")
                .max_height((ctx.content_rect().height() - 40.0).max(80.0))
                .show(ui, |ui| {
                    composer_intro(ui, app);
                    composer_content(ui, app);
                    composer_footer(ui, app);
                });
        } else {
            composer_intro(ui, app);
            egui::ScrollArea::vertical()
                .id_salt("composer_content")
                .max_height((ctx.content_rect().height() - 320.0).max(180.0))
                .show(ui, |ui| composer_content(ui, app));
            composer_footer(ui, app);
        }
    });
    if response.should_close() {
        app.composer_open = false;
    }
}

fn composer_intro(ui: &mut Ui, app: &App) {
    crate::ui::theme::badge(ui, "01 / ASK", app.theme.accent);
    crate::ui::theme::heading(ui, "Start a decision");
    ui.label(RichText::new("What are you choosing? The models suggest an idea, look for problems, and write a short guide. You decide what to do.").color(app.theme.text_muted));
    ui.add_space(10.0);
}

fn composer_content(ui: &mut Ui, app: &mut App) {
    question_editor(ui, &mut app.composer_state);
    ui.add_space(10.0);
    ui.label("Models are assigned to six roles. Review their availability and change any assignment below.");
    crate::ui::theme::disclosure(ui, "Change the models").show(ui, |ui| council_editor(ui, app));
}

fn composer_footer(ui: &mut Ui, app: &mut App) {
    ui.add_space(10.0);
    ui.separator();
    let blocker = start_blocker(&app.composer_state);
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut app.composer_state.dry_run, true, "Practice example");
        ui.selectable_value(&mut app.composer_state.dry_run, false, "Real provider run");
    });
    ui.label(
        RichText::new(if app.composer_state.dry_run {
            "Practice uses simulated responses. No inference request is sent to model providers."
        } else {
            "A real run sends your question to the model providers you picked and may cost money on their side."
        })
        .small()
        .color(app.theme.text_muted),
    );
    if let Some(message) = blocker {
        ui.label(RichText::new(message).color(app.theme.warning));
    }
    ui.add_space(4.0);
    let action_width = ui.available_width().min(170.0);
    let cancel_width = ui.available_width().min(110.0);
    ui.horizontal_wrapped(|ui| {
        let label = if app.is_running {
            "Review replacement run"
        } else {
            "Review run"
        };
        if ui
            .add_enabled(
                blocker.is_none(),
                crate::ui::theme::primary_button(&app.theme, label)
                    .min_size(egui::vec2(action_width, 44.0)),
            )
            .clicked()
        {
            app.submit_composer();
        }
        if ui
            .add_sized([cancel_width, 44.0], egui::Button::new("Cancel"))
            .clicked()
        {
            app.composer_open = false;
        }
        ui.label(
            RichText::new("Closes setup without starting a run. Your draft is kept.")
                .small()
                .color(app.theme.text_muted),
        );
    });
}

fn start_blocker(state: &ComposerState) -> Option<&'static str> {
    if state.question.trim().is_empty() {
        Some("Enter a decision question to continue.")
    } else if state.attachment.is_some() {
        Some("Wait for document selection or upload to finish.")
    } else if state
        .roles
        .values()
        .any(|role| role.model.trim().is_empty())
    {
        Some("Choose a model for each council role in Council settings.")
    } else {
        None
    }
}

fn question_editor(ui: &mut Ui, state: &mut ComposerState) {
    let question_label = ui.strong("What decision are you making?");
    ui.label("Optional: include the options, constraints, deadline, and what would change your decision.");
    ui.add(
        TextEdit::multiline(&mut state.question)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text(
                "For example: Should we open a second office in Nairobi?\nAdd your options, deadline, and anything the council should know.",
            ),
    ).labelled_by(question_label.id);
    document_editor(ui, state, cfg!(target_arch = "wasm32"));
}

fn document_editor(ui: &mut Ui, state: &mut ComposerState, browser: bool) {
    ui.add(egui::Label::new(crate::attachments::transfer_notice(browser)).wrap());
    let attachment_width = ui.available_width().min(170.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                state.attachment.is_none(),
                egui::Button::new("Add documents").min_size(egui::vec2(attachment_width, 44.0)),
            )
            .on_hover_text("Attach notes, numbers, or a document for the council to read")
            .clicked()
        {
            state.message.clear();
            state.attachment = Some(crate::attachments::choose());
        }
        ui.label(
            RichText::new("Optional. Notes, numbers, or documents.")
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
    selected_documents(ui, state, browser);
    if state.attachment.is_some() {
        ui.spinner();
        ui.label(if browser {
            "Choosing or uploading a document… Review is unavailable until this finishes."
        } else {
            "Waiting for document selection… Review is unavailable until this finishes."
        });
    }
    if !state.message.is_empty() {
        let message = ui.add(egui::Label::new(&state.message).wrap());
        ui.ctx().accesskit_node_builder(message.id, |node| {
            node.set_live(egui::accesskit::Live::Polite);
        });
    }
}

fn selected_documents(ui: &mut Ui, state: &mut ComposerState, browser: bool) {
    let mut remove = None;
    for (idx, file) in state.context_files.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Selected · Document {} ·", idx + 1));
            ui.label(crate::attachments::display_name(file));
            if ui.button("Remove").clicked() {
                remove = Some(idx);
            }
        });
    }
    if !state.context_files.is_empty() {
        crate::ui::theme::disclosure(ui, "Document storage references").show(ui, |ui| {
            for (index, file) in state.context_files.iter().enumerate() {
                ui.label(format!("Document {}: {file}", index + 1));
            }
        });
        ui.label("Review run checks document text before it can be included.");
        if browser {
            ui.label("Removing a document here excludes it from the run; it does not delete the server upload.");
        }
    }
    if let Some(idx) = remove {
        state.context_files.remove(idx);
        state.message = "Document removed from this draft. No run has started.".into();
    }
}

fn poll_attachment(state: &mut ComposerState) {
    let Some(rx) = &state.attachment else {
        return;
    };
    let result = match rx.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("Document selection ended without a result. Add the document again; existing inputs are unchanged.".into()),
    };
    state.message = match result {
        Ok(path) if state.context_files.contains(&path) => {
            "Document already selected. No duplicate was added.".into()
        }
        Ok(path) => {
            state.context_files.push(path);
            "Document selected. Review run will check its text.".into()
        }
        Err(message) => message,
    };
    state.attachment = None;
}

fn council_editor(ui: &mut Ui, app: &mut App) {
    ui.add(
        egui::Label::new(
            RichText::new(
                "Each role has a model assignment. Availability depends on provider access; a configured default is not a guarantee that a real request will succeed.",
            )
            .color(app.theme.text_muted),
        )
        .wrap(),
    );
    ui.add_space(6.0);
    let reload_width = ui.available_width().min(190.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !app.catalog_loading(),
                egui::Button::new("Reload the model lists")
                    .min_size(egui::vec2(reload_width, 44.0)),
            )
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
    ui.add_space(6.0);
    for (role, title) in ROLES {
        if !app.composer_state.roles.contains_key(role) {
            continue;
        }
        ui.push_id(role, |ui| role_row(ui, app, role, title));
    }
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.label("How many rounds of attack");
        ui.add(DragValue::new(&mut app.composer_state.rounds).range(1..=5))
            .accessible_name("Number of attack rounds");
    });
    ui.label(
        RichText::new(
            "More rounds means more arguing and more cost. One round is enough for most decisions.",
        )
        .small()
        .color(app.theme.text_muted),
    );
}

fn role_row(ui: &mut Ui, app: &mut App, role: &str, title: &str) {
    crate::ui::theme::card(&app.theme).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong());
        let layout = if ui.available_width() < 500.0 {
            egui::Layout::top_down(egui::Align::LEFT)
        } else {
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true)
        };
        ui.with_layout(layout, |ui| {
            let Some(selection) = app.composer_state.roles.get_mut(role) else {
                return;
            };
            ui.label(RichText::new("Model").small().color(app.theme.text_muted));
            provider_select(ui, selection, title);
            let catalog = app
                .catalog
                .iter()
                .find(|c| c.provider == selection.provider);
            model_select(
                ui,
                selection,
                catalog,
                app.composer_state.searches.entry(role.into()).or_default(),
                title,
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Model ID")
                    .small()
                    .color(app.theme.text_muted),
            );
            if let Some(selection) = app.composer_state.roles.get_mut(role) {
                ui.add(
                    TextEdit::singleline(&mut selection.model)
                        .desired_width(ui.available_width().min(390.0))
                        .hint_text("Type an exact model ID if it is not in the list"),
                )
                .accessible_name(format!("{title}: exact model ID"));
            }
        });
        if let Some(catalog) = app
            .catalog
            .iter()
            .find(|c| c.provider == app.composer_state.roles[role].provider)
            && !catalog.error.is_empty()
        {
            ui.add(egui::Label::new(RichText::new(&catalog.error).color(app.theme.warning)).wrap());
        }
    });
}

fn provider_select(ui: &mut Ui, selection: &mut Selection, title: &str) {
    let name = format!("{title}: provider, {}", selection.provider);
    egui::ComboBox::from_id_salt("provider")
        .width(ui.available_width().min(140.0))
        .truncate()
        .selected_text(&selection.provider)
        .show_ui(ui, |ui| {
            for provider in PROVIDERS {
                if ui
                    .selectable_value(&mut selection.provider, provider.into(), provider)
                    .changed()
                {
                    selection.model = roles::default_model(provider).into();
                    ui.close();
                }
            }
        })
        .response
        .accessible_name(name);
}

fn model_select(
    ui: &mut Ui,
    selection: &mut Selection,
    catalog: Option<&roles::ProviderCatalog>,
    search: &mut String,
    title: &str,
) {
    let name = format!("{title}: model, {}", selection.model);
    egui::ComboBox::from_id_salt("model")
        .width(ui.available_width().min(300.0))
        .truncate()
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
                        if ui
                            .selectable_value(&mut selection.model, model.clone(), model)
                            .changed()
                        {
                            ui.close();
                        }
                    }
                });
            ui.weak("You can also enter an exact model ID below.");
        })
        .response
        .accessible_name(name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::mpsc::channel;

    fn draft() -> ComposerState {
        ComposerState {
            question: "Synthetic attachment decision".into(),
            context_files: vec!["existing.pdf".into()],
            ..Default::default()
        }
    }

    #[test]
    fn interrupted_document_selection_unblocks_review_without_changing_the_draft() {
        let mut state = draft();
        let roles = state.roles.clone();
        let (tx, rx) = channel();
        state.attachment = Some(rx);
        poll_attachment(&mut state);
        assert!(state.attachment.is_some());
        assert!(
            start_blocker(&state)
                .unwrap()
                .contains("selection or upload")
        );
        drop(tx);
        poll_attachment(&mut state);
        assert!(state.attachment.is_none());
        assert!(start_blocker(&state).is_none());
        assert_eq!(state.question, "Synthetic attachment decision");
        assert_eq!(state.context_files, ["existing.pdf"]);
        assert_eq!(state.roles, roles);
        assert!(state.message.contains("ended without a result"));
    }

    #[test]
    fn selection_errors_cancellation_and_repeated_paths_preserve_existing_inputs() {
        for result in [
            Err("No file selected. Existing documents are unchanged.".into()),
            Err("Synthetic server storage failure. Add the document again.".into()),
            Ok("existing.pdf".into()),
        ] {
            let mut state = draft();
            let (tx, rx) = channel();
            state.attachment = Some(rx);
            tx.send(result).unwrap();
            poll_attachment(&mut state);
            assert!(state.attachment.is_none());
            assert!(start_blocker(&state).is_none());
            assert_eq!(state.context_files, ["existing.pdf"]);
            assert_eq!(state.question, "Synthetic attachment decision");
            assert!(!state.message.is_empty());
        }
    }

    #[test]
    fn successful_selection_is_consumed_once_and_requires_text_review() {
        let mut state = draft();
        let (tx, rx) = channel();
        state.attachment = Some(rx);
        tx.send(Ok("synthetic-not-yet-read.txt".into())).unwrap();
        poll_attachment(&mut state);
        poll_attachment(&mut state);
        assert_eq!(
            state.context_files,
            ["existing.pdf", "synthetic-not-yet-read.txt"]
        );
        assert!(state.message.contains("Review run will check its text"));
        assert!(state.attachment.is_none());
    }

    fn find<'a>(tree: &'a Value, predicate: &impl Fn(&Value) -> bool) -> Option<&'a Value> {
        if predicate(tree) {
            return Some(tree);
        }
        tree["children"]
            .as_array()?
            .iter()
            .find_map(|node| find(node, predicate))
    }

    #[test]
    fn browser_disclosure_precedes_selection_and_reports_pending_and_failed_states() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_zoom_factor(1.5);
        crate::ui::theme::Theme::default().apply(&ctx);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0 / 1.5, 900.0 / 1.5),
            )),
            ..Default::default()
        };
        // Apply egui's pending zoom before measuring the fixed physical viewport.
        for _ in 0..3 {
            let mut warmup = ctx.run_ui(input(), |_| {});
            warmup.textures_delta.clear();
        }
        let mut state = draft();
        let (tx, rx) = channel();
        state.attachment = Some(rx);
        let mut tree = Value::Null;
        let mut output = ctx.run_ui(input(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| document_editor(ui, &mut state, true));
            tree = serde_json::to_value(crate::accessibility::snapshot(&ctx)).unwrap();
        });
        output.textures_delta.clear();
        for shape in &output.shapes {
            if let egui::epaint::Shape::Text(text) = &shape.shape {
                assert!(
                    text.pos.x + text.galley.size().x <= 320.0 / 1.5 + 0.5,
                    "document controls exceed the narrow viewport: {} (right {}, content {:?}, zoom {})",
                    text.galley.text(),
                    text.pos.x + text.galley.size().x,
                    ctx.content_rect(),
                    ctx.zoom_factor()
                );
            }
        }
        let add = find(&tree, &|node| node["label"] == "Add documents").unwrap();
        assert_eq!(add["disabled"], true);
        let content = tree.to_string();
        assert!(content.contains("uploads it to the Council server, even in practice mode"));
        assert!(content.contains("does not delete the server upload"));
        assert!(content.contains("checks document text before it can be included"));
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => Some((
                    text.galley.text(),
                    text.pos.y + text.galley.size().y,
                    text.pos.y,
                )),
                _ => None,
            })
            .collect();
        let notice = labels
            .iter()
            .find(|(text, _, _)| text.contains("Selecting a file uploads"))
            .unwrap();
        let button = labels
            .iter()
            .find(|(text, _, _)| *text == "Add documents")
            .unwrap();
        assert!(notice.1 <= button.2);
        drop(tx);
        poll_attachment(&mut state);
        let mut recovered = Value::Null;
        let mut recovered_output = ctx.run_ui(input(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| document_editor(ui, &mut state, true));
            recovered = serde_json::to_value(crate::accessibility::snapshot(&ctx)).unwrap();
        });
        recovered_output.textures_delta.clear();
        assert_eq!(
            find(&recovered, &|node| node["label"] == "Add documents").unwrap()["disabled"],
            false
        );
        assert!(
            find(&recovered, &|node| node["live"] == true
                && node["value"].as_str().is_some_and(
                    |value| value.contains("ended without a result")
                ))
            .is_some()
        );
    }
}
