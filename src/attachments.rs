use std::sync::mpsc::{Receiver, channel};

pub fn choose() -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        let selection = rfd::FileDialog::new()
            .add_filter("Context", &["txt", "md", "csv", "pdf", "docx", "xlsx"])
            .pick_file();
        let _ = tx.send(
            selection
                .map(|p| p.to_string_lossy().into_owned())
                .ok_or_else(|| "No file selected".into()),
        );
    });
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let Some(file) = rfd::AsyncFileDialog::new().pick_file().await else {
            let _ = tx.send(Err("No file selected".into()));
            return;
        };
        let name = file.file_name();
        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let body = file.read().await;
        ehttp::fetch(
            ehttp::Request::post(format!("/api/context?name={safe_name}"), body),
            move |result| {
                let path = result.map_err(|e| e.to_string()).and_then(|r| {
                    if r.ok {
                        String::from_utf8(r.bytes).map_err(|e| e.to_string())
                    } else {
                        Err("Attachment could not be read (10 MB maximum).".into())
                    }
                });
                let _ = tx.send(path);
            },
        );
    });
    rx
}
