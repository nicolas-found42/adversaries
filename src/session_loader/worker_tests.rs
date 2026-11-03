use super::{load_session_async, read_on_worker, scan_sessions_async};
use std::{sync::mpsc::channel, time::Duration};

#[test]
fn a_blocked_storage_operation_does_not_block_its_requester() {
    let (release, gate) = channel();
    let (requested, returned) = channel();
    let requester = std::thread::spawn(move || {
        let receiver = read_on_worker(move || {
            gate.recv().unwrap();
            "Storage result"
        });
        requested.send(receiver).unwrap();
    });
    let receiver = returned
        .recv_timeout(Duration::from_secs(5))
        .expect("requester blocked on storage");
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    release.send(()).unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
        "Storage result"
    );
    requester.join().unwrap();
}

#[test]
fn native_workers_use_the_same_saved_file_parser_and_preserve_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.json");
    std::fs::write(
        &path,
        r#"{"question":"Synthetic café?","brief":"A retained recommendation.","turns":[]}"#,
    )
    .unwrap();
    let list = scan_sessions_async(dir.path().to_path_buf())
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].question, "Synthetic café?");
    assert!(list[0].has_brief);
    let detail = load_session_async(path.to_string_lossy().into_owned())
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(detail.brief, "A retained recommendation.");
    assert!(
        scan_sessions_async(path)
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_err()
    );
}
