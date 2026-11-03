use council_ui::app::App;
use council_ui::ui;

#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// Shared eframe shell around the framework-independent [`App`].
struct CouncilApp {
    app: App,
}

impl CouncilApp {
    fn new(demo_mode: bool, question: Option<String>, dry_run: bool) -> Self {
        let mut app = App::new(demo_mode, question, dry_run);
        app.refresh_sessions();
        #[cfg(target_arch = "wasm32")]
        let app = {
            let mut app = app;
            if prefers_reduced_motion() {
                app.theme.motion_seconds = 0.0;
            }
            app
        };
        Self { app }
    }
}

/// Reads the operating system's reduced-motion setting. Only the browser build
/// can ask for it; the native window keeps egui's default animation.
#[cfg(target_arch = "wasm32")]
fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(prefers-reduced-motion: reduce)").ok())
        .flatten()
        .is_some_and(|query| query.matches())
}

impl eframe::App for CouncilApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        ui::sculpture::destroy();
    }

    #[cfg(target_arch = "wasm32")]
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        council_ui::accessibility::input(ctx, raw);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.tick();
        self.app.handle_keys(ctx);
        if self.app.needs_poll() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(not(target_arch = "wasm32"))]
        ui::render(ui, &mut self.app);
        #[cfg(target_arch = "wasm32")]
        {
            let workspace = ui::render_with_workspace(ui, &mut self.app);
            council_ui::accessibility::output(ui.ctx(), &workspace.value().to_string());
        }
        if self.app.should_quit {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Parser, Debug)]
#[command(
    name = "council-ui",
    author,
    version,
    about = "Interactive egui UI for Adversarial Council multi-model decision debates"
)]
struct Cli {
    /// Optional decision question to start immediately
    question: Option<String>,

    /// Run in offline interactive demo mode (zero API keys needed)
    #[arg(long)]
    demo: bool,

    /// Run council backend in dry-run mode (no API cost)
    #[arg(long)]
    dry_run: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let cli = Cli::parse();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Adversarial Council")
            .with_inner_size([1280.0, 800.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Adversarial Council",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(CouncilApp::new(
                cli.demo,
                cli.question,
                cli.dry_run,
            )))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no browser window")
            .document()
            .expect("no DOM document");
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("missing #the_canvas_id in index.html")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("element is not a canvas");
        let web_options = eframe::WebOptions {
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        };
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_cc| Ok(Box::new(CouncilApp::new(false, None, false)))),
            )
            .await
            .expect("eframe web start failed");
        if let Some(loading) = document.get_element_by_id("loading_text") {
            loading.remove();
        }
    });
}
