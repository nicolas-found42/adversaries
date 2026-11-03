use council_ui::app::App;

pub fn finish_storage(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.session_opening() || app.history_loading() {
        app.tick();
        assert!(
            std::time::Instant::now() < deadline,
            "saved-file fixture did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
