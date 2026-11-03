use std::sync::mpsc::{Receiver, channel};

pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
pub const INVALID_DOCUMENT_NAME: &str =
    "Document filename contains unsupported characters. Rename the file and choose it again.";

pub fn valid_document_name(name: &str) -> bool {
    !name.trim().is_empty()
        && !matches!(name, "." | "..")
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
}

pub fn display_name(path: &str) -> &str {
    let windows_absolute = path.starts_with("\\\\")
        || (path.as_bytes().get(1) == Some(&b':') && path.as_bytes().get(2) == Some(&b'\\'));
    if windows_absolute {
        path.rsplit(['/', '\\']).next().unwrap_or(path)
    } else {
        path.rsplit('/').next().unwrap_or(path)
    }
}

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
                .ok_or_else(|| "No file selected. Existing documents are unchanged.".into()),
        );
    });
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tx.send(upload_selected().await);
    });
    rx
}

pub fn transfer_notice(browser: bool) -> &'static str {
    if browser {
        "Selecting a file uploads it to the Council server, even in practice mode. Maximum 10 MiB per file. Review the extracted text before sending it to providers."
    } else {
        "Selected files are read locally. Review the extracted text before sending it to providers in a real run."
    }
}

#[cfg(target_arch = "wasm32")]
async fn upload_selected() -> Result<String, String> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Context", &["txt", "md", "csv", "pdf", "docx", "xlsx"])
        .pick_file()
        .await
        .ok_or_else(|| "No file selected. Existing documents are unchanged.".to_owned())?;
    check_upload_size(file.inner().size())?;
    let buffer = wasm_bindgen_futures::JsFuture::from(file.inner().array_buffer())
        .await
        .map_err(|_| {
            "Could not read the selected file. Choose it again or select another document."
                .to_owned()
        })?;
    let bytes = web_sys::js_sys::Uint8Array::new(&buffer).to_vec();
    let name = file.file_name();
    if !valid_document_name(&name) {
        return Err(INVALID_DOCUMENT_NAME.into());
    }
    let name = String::from(web_sys::js_sys::encode_uri_component(&name));
    let response = ehttp::fetch_async(crate::transport::post_json(
        format!("/api/context?name={name}"),
        bytes,
    ))
    .await
    .map_err(|_| {
        "Could not reach the Council server. Check the connection and add the document again."
            .to_owned()
    })?;
    upload_response(response.status, response.bytes)
}

#[cfg(any(target_arch = "wasm32", test))]
fn check_upload_size(bytes: f64) -> Result<(), String> {
    if bytes > MAX_UPLOAD_BYTES as f64 {
        Err("Document exceeds the 10 MiB upload limit. Choose a smaller file.".into())
    } else {
        Ok(())
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn upload_response(status: u16, bytes: Vec<u8>) -> Result<String, String> {
    match status {
        200..=299 => {
            let path = String::from_utf8(bytes).map_err(|_| "The server returned an invalid document reference. Add the document again.".to_owned())?;
            if path.trim().is_empty() {
                return Err("The server returned no document reference. Add the document again.".into());
            }
            Ok(path)
        }
        413 => Err("Document exceeds the 10 MiB upload limit. Choose a smaller file.".into()),
        500..=599 => Err("The Council server could not store the document. Retry or choose another document; your draft is unchanged.".into()),
        _ => Err(format!("Document upload was rejected (HTTP {status}). Add the document again; your draft is unchanged.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploaded_names_preserve_unicode_and_punctuation_without_accepting_paths() {
        for name in [
            "Planning café & costs #1 + 100%.txt",
            "同じ名前.pdf",
            "report_2026.final.docx",
        ] {
            assert!(valid_document_name(name), "{name}");
        }
        for name in [
            "",
            " ",
            ".",
            "..",
            "../escape.txt",
            "C:escape.txt",
            "folder/file.pdf",
            r"folder\file.pdf",
            "line\nbreak.txt",
            "nul\0.txt",
        ] {
            assert!(!valid_document_name(name), "{name:?}");
        }
    }

    #[test]
    fn display_names_understand_absolute_server_paths_without_guessing_local_backslashes() {
        assert_eq!(
            display_name("/server/.council-context/id/Planning café.txt"),
            "Planning café.txt"
        );
        assert_eq!(
            display_name(r"C:\Council\.council-context\id\Planning café.txt"),
            "Planning café.txt"
        );
        assert_eq!(
            display_name(r"\\server\share\Planning café.txt"),
            "Planning café.txt"
        );
        assert_eq!(
            display_name(r"/local/notes\literal.txt"),
            r"notes\literal.txt"
        );
        assert_eq!(
            display_name("1793_recorded-legacy.txt"),
            "1793_recorded-legacy.txt"
        );
    }

    #[test]
    fn upload_limit_matches_the_server_boundary() {
        assert!(check_upload_size(MAX_UPLOAD_BYTES as f64).is_ok());
        assert!(
            check_upload_size((MAX_UPLOAD_BYTES + 1) as f64)
                .unwrap_err()
                .contains("10 MiB")
        );
    }

    #[test]
    fn upload_failure_does_not_masquerade_as_a_file_size_error() {
        assert!(upload_response(413, vec![]).unwrap_err().contains("10 MiB"));
        assert!(
            upload_response(500, vec![])
                .unwrap_err()
                .contains("could not store")
        );
        assert!(
            upload_response(403, vec![])
                .unwrap_err()
                .contains("HTTP 403")
        );
    }

    #[test]
    fn only_a_successful_nonempty_utf8_reference_can_be_selected() {
        let path = "/server/context/synthetic_é.pdf";
        assert_eq!(
            upload_response(200, path.as_bytes().to_vec()).unwrap(),
            path
        );
        assert!(upload_response(200, vec![0xff]).is_err());
        assert!(upload_response(200, b" \n".to_vec()).is_err());
    }
}
