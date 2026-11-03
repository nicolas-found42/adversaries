//! Shared egui semantics for the browser's optional HTML reading/control view.
//! No separate decision model: HTML actions enter egui's existing AccessKit path.
use egui::accesskit::{Action, Node, NodeId, Role};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Debug)]
pub struct SemanticNode {
    id: String,
    role: String,
    level: Option<usize>,
    horizontal: bool,
    live: bool,
    label: String,
    value: String,
    placeholder: String,
    disabled: bool,
    readonly: bool,
    toggled: Option<bool>,
    expanded: Option<bool>,
    click: bool,
    set_value: bool,
    numeric: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document: Option<crate::ui::reader::document::Document>,
    children: Vec<Self>,
}

fn collect(ctx: &egui::Context, id: NodeId, nodes: &mut HashMap<NodeId, Node>) {
    // These bits came directly from egui Id::accesskit_id(), not an external hash.
    let egui_id = unsafe { egui::Id::from_high_entropy_bits(id.0) };
    let Some(node) = ctx.accesskit_node_builder(egui_id, |node| node.clone()) else {
        return;
    };
    let children = node.children().to_vec();
    nodes.insert(id, node);
    for child in children {
        collect(ctx, child, nodes);
    }
}

fn name(node: &Node, nodes: &HashMap<NodeId, Node>) -> String {
    node.label().map_or_else(
        || {
            node.labelled_by()
                .iter()
                .filter_map(|id| nodes.get(id))
                .filter_map(|label| label.value().or_else(|| label.label()))
                .collect::<Vec<_>>()
                .join(" ")
        },
        str::to_owned,
    )
}

fn view(ctx: &egui::Context, id: NodeId, nodes: &HashMap<NodeId, Node>) -> Option<SemanticNode> {
    let node = nodes.get(&id)?;
    if node.is_hidden() || node.role() == Role::TextRun {
        return None;
    }
    let document = crate::ui::reader::document::projection(ctx, id.0);
    let children = if document.is_some() {
        Vec::new()
    } else {
        node.children()
            .iter()
            .filter_map(|id| view(ctx, *id, nodes))
            .collect()
    };
    Some(SemanticNode {
        id: id.0.to_string(),
        role: format!("{:?}", node.role()),
        level: node.level(),
        horizontal: node.orientation() == Some(egui::accesskit::Orientation::Horizontal),
        live: node.live() == Some(egui::accesskit::Live::Polite),
        label: name(node, nodes),
        value: node.value().unwrap_or_default().into(),
        placeholder: node.placeholder().unwrap_or_default().into(),
        disabled: node.is_disabled(),
        readonly: node.is_read_only(),
        toggled: node
            .toggled()
            .map(|value| value == egui::accesskit::Toggled::True),
        expanded: node.is_expanded(),
        click: node.supports_action(Action::Click),
        set_value: node.supports_action(Action::SetValue),
        numeric: node.numeric_value(),
        min: node.min_numeric_value(),
        max: node.max_numeric_value(),
        step: node.numeric_value_step(),
        document,
        children,
    })
}

/// Panel painting order reserves footer space before the main view. Reading
/// order instead follows navigation, content, status, then any overlays.
pub(crate) fn order_landmarks(ui: &egui::Ui) {
    let ctx = ui.ctx();
    let root = ui.unique_id();
    let Some(mut children) = ctx.accesskit_node_builder(root, |node| node.children().to_vec())
    else {
        return;
    };
    children.sort_by_key(|id| panel_rank(ctx, *id));
    ctx.accesskit_node_builder(root, |node| node.set_children(children));
}

fn panel_rank(ctx: &egui::Context, id: NodeId) -> u8 {
    let id = unsafe { egui::Id::from_high_entropy_bits(id.0) };
    let Some((role, children)) =
        ctx.accesskit_node_builder(id, |node| (node.role(), node.children().to_vec()))
    else {
        return 3;
    };
    if role != Role::Pane {
        return landmark_rank(role);
    }
    children
        .iter()
        .filter_map(|id| {
            let id = unsafe { egui::Id::from_high_entropy_bits(id.0) };
            ctx.accesskit_node_builder(id, |node| landmark_rank(node.role()))
        })
        .min()
        .unwrap_or(3)
}

fn landmark_rank(role: Role) -> u8 {
    match role {
        Role::Navigation => 0,
        Role::Main => 1,
        Role::ContentInfo => 2,
        _ => 3,
    }
}

/// Snapshot during the UI pass, before eframe consumes its platform output.
/// Includes normal offscreen content; AccessKit excludes explicitly hidden UI.
pub fn snapshot(ctx: &egui::Context) -> Option<SemanticNode> {
    let root = egui::accesskit_root_id().accesskit_id();
    if !ctx.has_accesskit_node(egui::accesskit_root_id()) {
        return None;
    }
    let mut nodes = HashMap::new();
    collect(ctx, root, &mut nodes);
    view(ctx, root, &nodes)
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::snapshot;
    use egui::accesskit::{Action, ActionData, ActionRequest, TreeId};
    use serde::Deserialize;
    use wasm_bindgen::prelude::*;

    thread_local! { static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

    #[wasm_bindgen(module = "/src/web_accessibility.js")]
    extern "C" {
        #[wasm_bindgen(js_name = syncTree)]
        fn sync_tree(tree: &str, focused: &str, workspace: &str, scale: f32);
        #[wasm_bindgen(js_name = takeActions)]
        fn take_actions() -> String;
        #[wasm_bindgen(js_name = isActive)]
        fn is_active() -> bool;
        #[wasm_bindgen(js_name = setWake)]
        fn set_wake(callback: &JsValue);
    }

    #[derive(Deserialize)]
    struct Request {
        id: String,
        action: String,
        value: Option<String>,
    }

    impl Request {
        fn event(self) -> Option<egui::Event> {
            if self.action == "escape" {
                return Some(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let (action, data) = match self.action.as_str() {
                "click" => (Action::Click, None),
                "focus" => (Action::Focus, None),
                "value" => (
                    Action::SetValue,
                    Some(ActionData::Value(self.value?.into())),
                ),
                "numeric" => (
                    Action::SetValue,
                    Some(ActionData::NumericValue(self.value?.parse().ok()?)),
                ),
                _ => return None,
            };
            Some(egui::Event::AccessKitActionRequest(ActionRequest {
                action,
                target_tree: TreeId::ROOT,
                target_node: self.id.parse::<u64>().ok()?.into(),
                data,
            }))
        }
    }

    fn install(ctx: &egui::Context) {
        INSTALLED.with(|installed| {
            if installed.replace(true) {
                return;
            }
            let ctx = ctx.clone();
            let callback = Closure::<dyn FnMut()>::new(move || ctx.request_repaint());
            set_wake(callback.as_ref());
            callback.forget(); // Browser adapter lives for the WebRunner lifetime.
        });
    }

    pub fn input(ctx: &egui::Context, raw: &mut egui::RawInput) {
        install(ctx);
        ctx.enable_accesskit();
        let requests: Vec<Request> = serde_json::from_str(&take_actions()).unwrap_or_default();
        raw.events
            .extend(requests.into_iter().filter_map(Request::event));
        if raw.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Escape,
                    ..
                }
            )
        }) {
            raw.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        if is_active() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub fn output(ctx: &egui::Context, workspace: &str) {
        if let Some(tree) = snapshot(ctx)
            && let Ok(json) = serde_json::to_string(&tree)
        {
            let focused = ctx
                .memory(egui::Memory::focused)
                .map(|id| id.value().to_string())
                .unwrap_or_default();
            sync_tree(&json, &focused, workspace, ctx.zoom_factor());
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::{input, output};
