//! Document extraction and credential checks never perform inference.
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, channel};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentSnapshot {
    pub name: String,
    pub text: String,
    pub text_digest: String,
    pub sha256: String,
    pub original_characters: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentCheck {
    pub snapshot: DocumentSnapshot,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCheck {
    pub name: String,
    pub configured: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreflightReport {
    pub ready: bool,
    pub documents: Vec<DocumentCheck>,
    pub providers: Vec<ProviderCheck>,
    pub error: String,
}

pub fn check(config: crate::transport::RunConfig) -> Receiver<Result<PreflightReport, String>> {
    let (tx, rx) = channel();
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        let _ = tx.send(check_native(&config));
    });
    #[cfg(target_arch = "wasm32")]
    {
        let body = serde_json::to_vec(&config).expect("serializable plan");
        ehttp::fetch(
            crate::transport::post_json("/api/preflight", body),
            move |result| {
                let result = result.map_err(|e| e.to_string()).and_then(|r| {
                    if !r.ok {
                        return Err(
                            "Could not check inputs. Retry preflight or edit the plan.".into()
                        );
                    }
                    serde_json::from_slice(&r.bytes).map_err(|e| e.to_string())
                });
                let _ = tx.send(result);
            },
        );
    }
    rx
}

#[cfg(not(target_arch = "wasm32"))]
pub fn check_native(config: &crate::transport::RunConfig) -> Result<PreflightReport, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let (program, args) = crate::transport::engine_json_command(true);
    let mut child = Command::new(program)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not check inputs: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("Input pipe unavailable")?
        .write_all(&serde_json::to_vec(config).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Input check failed. Check that the engine and document readers are installed, then retry.".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid input-check response: {e}"))
}
