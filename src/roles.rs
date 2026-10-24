//! Role routing and provider catalogs, shared by native and browser surfaces.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, channel};

pub const PROVIDERS: [&str; 4] = ["Mistral", "OpenRouter", "Ollama-Cloud", "OpenCode-Go"];
pub const ROLES: [(&str, &str); 6] = [
    ("opening", "Opening analyst"),
    ("critique_1", "First critique"),
    ("critique_2", "Second critique"),
    ("critique_3", "Third critique"),
    ("rebuttal", "Rebuttal"),
    ("brief", "Decision brief"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selection {
    pub provider: String,
    pub model: String,
}

pub fn default_model(provider: &str) -> &str {
    match provider {
        "Mistral" => "mistral-small-latest",
        "OpenRouter" => "meta-llama/llama-3.3-70b-instruct",
        _ => "deepseek-v4.1-flash",
    }
}

pub fn defaults() -> BTreeMap<String, Selection> {
    ROLES
        .iter()
        .zip([0, 1, 2, 3, 0, 0])
        .map(|((role, _), idx)| {
            let provider = PROVIDERS[idx];
            (
                (*role).into(),
                Selection {
                    provider: provider.into(),
                    model: default_model(provider).into(),
                },
            )
        })
        .collect()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCatalog {
    pub provider: String,
    #[serde(default)]
    pub default_model: String,
    pub models: Vec<String>,
    pub ready: bool,
    pub error: String,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_catalog() -> Result<Vec<ProviderCatalog>, String> {
    let output = std::process::Command::new("uv")
        .args(["run", "council.py", "--models-json"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "Could not load provider catalogs. Check backend setup.".into())
}

pub fn fetch_catalog() -> Receiver<Result<Vec<ProviderCatalog>, String>> {
    let (tx, rx) = channel();
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        let _ = tx.send(load_catalog());
    });
    #[cfg(target_arch = "wasm32")]
    ehttp::fetch(ehttp::Request::get("/api/models"), move |result| {
        let catalog = result.map_err(|e| e.to_string()).and_then(|r| {
            serde_json::from_slice(&r.bytes).map_err(|_| "Could not load provider catalogs".into())
        });
        let _ = tx.send(catalog);
    });
    rx
}
