# Rust GUI research: one pretty UI, native and browser, live-reloaded

**Date:** 2026-10-23
**Scope:** how to replace the Ratatui TUI and the Vite/React app with one Rust GUI codebase that (a) runs natively, (b) can be served in a browser by a dev server that rebuilds on Rust edits, and (c) can still drive `uv run council.py --json-stream` for real debates. Covers ratatui, egui/eframe, trunk, wasm-server-runner, hot-lib-reloader, Dioxus, Slint, and iced, plus the wasm-cannot-spawn-processes constraint.
**Method:** primary sources only — docs.rs rustdoc pages (including rustdoc-JSON renders), the crate registry (`cargo info` / `cargo search`), official docs sites (`trunk-rs.github.io`, `dioxuslabs.com/learn`, `docs.slint.dev`, `ratatui.rs`, `book.iced.rs`, `doc.rust-lang.org`), `raw.githubusercontent.com` READMEs, and this repo's source. No blog posts or third-party summaries were used as authority. All URLs accessed **2026-10-23**; every version below was read from the registry or docs.rs on that date.

---

## Bottom line

1. **Ratatui cannot render to a browser by itself.** Its documented backends are terminal emulators only — Crossterm, Termion, Termwiz, Termina, plus a test backend — and the backend trait exists to "interface with the terminal emulator" ([ratatui backends](https://ratatui.rs/concepts/backends/) — "Ratatui interfaces with the terminal emulator through a backend."). There is no web backend in `ratatui 0.30.2`: the `backend` module lists only `CrosstermBackend`, `TerminaBackend`, `TermionBackend`, `TermwizBackend`, `TestBackend` ([docs.rs](https://docs.rs/ratatui/0.30.2/ratatui/backend/index.html) — "A `Backend` implementation that uses Crossterm to render to the terminal.").
2. **A browser path for ratatui exists, but it is terminal-shaped.** Ratzilla — from the ratatui organisation — is "terminal-themed web applications with Rust and WebAssembly" with a `DomBackend`/`CanvasBackend`, served by `trunk serve` ([docs.rs/ratzilla](https://docs.rs/ratzilla/0.3.1/ratzilla/) — "Build terminal-themed web applications with Rust and WebAssembly. Powered by Ratatui."). It buys browser delivery, not a prettier UI, and it still cannot start a process in the page.
3. **The lowest-risk "same code, native + browser" stack is egui/eframe + trunk.** eframe's own docs say you implement `App` once and then call `run_native` and/or use `WebRunner` ([docs.rs/eframe](https://docs.rs/eframe/0.37.0/eframe/) — "you implement App … and then call `crate::run_native` from your `main.rs`, and/or use `eframe::WebRunner` from your `lib.rs`"), and `trunk serve` "triggers a rebuild and a refresh in the browser" ([Trunk configuration](https://trunk-rs.github.io/trunk/guide/configuration/) — "Trunk has built-in support for watching for source file changes, which triggers a rebuild and a refresh in the browser.").
4. **`trunk serve` is a live-reload loop, not hot code replacement.** It rebuilds the wasm and refreshes the page; a refresh instantiates a fresh wasm module, so in-page state does not survive [INFERENCE]. True state-preserving hot reload on native is hot-lib-reloader's job, and it is dylib-based, so it is native-only and needs `codesign` on macOS ([hot-lib-reloader](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/) — "On macOS the reloadable library needs to get codesigned.").
5. **Dioxus has the most integrated Rust hot-reload story, but it is mid-release-cycle.** `dx serve` "Build, watch, and serve the project", RSX hot-reload needs no recompile, and `dx serve --hotpatch` enables experimental Rust hot-patching via Subsecond ([Dioxus hot-reload reference](https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload/) — "Dioxus 0.7 introduces experimental Rust hot-reloading"). Stable is 0.7.10 (2026-07-30) while 0.8.0-alpha.3 shipped 2026-10-16 ([docs.rs/crate/dioxus](https://docs.rs/crate/dioxus/0.7.10)).
6. **Slint's live preview is the best UI-iteration loop of the set, but it is a different language and a different licence conversation.** `.slint` files reload in the running app with state preserved ([Slint live preview](https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview/) — "The live preview feature lets your running application watch .slint files on disk and reload them automatically when they change."), and the crate is triple-licensed GPL-3.0-only / Royalty-free-2.0 / Commercial ([crates.io](https://crates.io/crates/slint/1.18.1)).
7. **iced claims web support and maintains wasm code, but ships no web setup guide.** The README lists "Cross-platform support (Windows, macOS, Linux, and the Web)", the examples README says the tour "can run both on native platforms and the web!", and 0.14's changelog fixes "WebGPU failing to boot in Chromium" — yet the project still calls itself "currently experimental software" and no trunk/`wasm-bindgen` recipe was found ([README](https://raw.githubusercontent.com/iced-rs/iced/master/README.md)).
8. **The hard constraint is real and documented.** On `wasm32-unknown-unknown`, "many parts of the standard library do not work and return errors… `std::fs` always return errors, and `std::thread::spawn` will panic" ([rustc book](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)). A wasm UI therefore cannot run `uv run council.py`; something native must own the child process.
9. **The documented client for that native thing is `ehttp`** — "Minimal HTTP client for both native and WASM", listed by eframe as a companion crate ([docs.rs/ehttp](https://docs.rs/ehttp/0.7.2/ehttp/); [eframe](https://docs.rs/crate/eframe/0.37.0) — "HTTP client: ehttp and reqwest"). Its `streaming` feature delivers the response "in parts … as they are streamed in", which is the hook for Server-Sent Events ([ehttp::streaming](https://docs.rs/ehttp/0.7.2/ehttp/streaming/index.html)).
10. **Recommendation:** keep one egui UI crate, add a small **axum sidecar bridge** that spawns `council.py` and streams `BackendEvent` JSON as SSE, serve the wasm build with `trunk serve` (its proxy removes CORS in dev), keep direct process spawning for the native build, and delete both the Ratatui and React UIs after cutover. Details and the migration map are below.

---

## What this repo constrains

| Constraint | Evidence |
|---|---|
| One engine, spawned as a child process, JSON lines on stdout | `src/backend.rs:91-92` (`Command::new("uv").arg("run").arg("council.py").arg("--json-stream")`); `stream_stdout` parses each line into `BackendEvent` |
| Event/model types already serde-serialisable (reusable over HTTP) | `src/model.rs` — `BackendEvent` is `#[serde(tag = "type", rename_all = "snake_case")]` |
| Five views driven by one `Tab` enum | `src/model.rs` (`Tab::{Debate, Brief, Claims, Sessions, Help}`); `src/app.rs:207-211` binds keys `1..5` |
| Hand-rolled theme table (15 colours, per-model colours) | `src/ui/theme.rs` (`Theme { bg, card_bg, border, … mistral, openrouter, ollama, opencode }`) |
| Brief rendered by a hand-written Markdown-ish line styler | `src/ui/brief_view.rs` (`format_brief_content` handles `#`, `##`, `- `, `|`) |
| Sessions come from scanning `reports/` on disk | `src/app.rs:36-37` (`scan_sessions(&PathBuf::from("reports"))`), `src/session_loader.rs` |
| Composer attaches local files | `src/ui/composer.rs`, `src/model.rs` (`context_files`), passed as `--context` in `src/backend.rs` |
| A second UI already duplicates this in Node | `web/vite.config.ts:148-153` spawns the same engine; deletion candidate |

The last two rows are why the browser build cannot simply be "the same app with a canvas": file pickers and session history need a native owner.

---

## Option 1 — Ratatui in the browser

**Verdict: no for a "pretty" browser UI; yes only as a terminal-themed web page.**

- Backends are terminal emulators, enumerated in the official backends page and in the 0.30.2 API: Crossterm, Termion, Termwiz, Termina, and `TestBackend` ([ratatui.rs/concepts/backends](https://ratatui.rs/concepts/backends/) — "Each backend supports Raw Mode… Alternate Screen… Mouse Capture"; [docs.rs/ratatui/0.30.2/ratatui/backend](https://docs.rs/ratatui/0.30.2/ratatui/backend/index.html)).
- The `Backend` trait is defined as "an abstraction over different terminal libraries" ([docs.rs](https://docs.rs/ratatui/0.30.2/ratatui/backend/trait.Backend.html) — "provides an abstraction over different terminal libraries").
- Ratzilla is the closest thing to a web backend: `DomBackend` plus `CanvasBackend`, packaged with trunk (`<link data-trunk rel="rust"/>`), giving "TUIs in your browser" ([docs.rs/ratzilla](https://docs.rs/ratzilla/0.3.1/ratzilla/) — "Now go to http://localhost:8080 and enjoy TUIs in your browser!"). It is a separate crate (`ratzilla 0.3.1`), not `ratatui` itself.
- Even with Ratzilla, the browser page is still a terminal grid: block borders, monospace cells, key events. That does not meet "one pretty Rust GUI".
- Any wasm page still cannot spawn the engine (see the constraint section), so Ratzilla would need the same bridge as egui.

**Keep or replace:** replace the Ratatui implementation for the browser goal. Its state machine (`src/app.rs`) and model (`src/model.rs`) are the valuable parts; the drawing layer is replaceable.

---

## Option 2 — egui / eframe

**Verdict: primary candidate.**

- egui is "An easy-to-use immediate mode GUI that runs on both web and native" ([crates.io/egui](https://crates.io/crates/egui/0.37.0)).
- One `App` implementation serves both targets. Native entry: `eframe::run_native(...)`; web entry: `eframe::WebRunner::new().start(canvas, WebOptions::default(), ...)` ([docs.rs/eframe](https://docs.rs/eframe/0.37.0/eframe/) — code samples for both, plus "You implement App (especially App::ui)").
- eframe's crate page is explicit: "The app can be compiled both to run natively (for Linux, Mac, Windows, and Android) or as a web app (using Wasm)" ([docs.rs/crate/eframe/0.37.0](https://docs.rs/crate/eframe/0.37.0)).
- The canonical scaffold is `eframe_template`, which "will be updated in lock-step to always use the latest version of egui" ([README](https://raw.githubusercontent.com/emilk/eframe_template/main/README.md)) — but on 2026-10-23 it still pins `egui = "0.36.0"` / `eframe = "0.36.0"` ([Cargo.toml](https://raw.githubusercontent.com/emilk/eframe_template/main/Cargo.toml)) while the registry has 0.37.0 (released 2026-10-15, [docs.rs/crate/eframe/0.37.0](https://docs.rs/crate/eframe/0.37.0)).
- The web side is a `<canvas id="the_canvas_id">` plus a `<link data-trunk rel="rust" data-wasm-opt="2" />`, with a small `wasm-bindgen-futures` bootstrap in `main.rs` under `#[cfg(target_arch = "wasm32")]` ([index.html](https://raw.githubusercontent.com/emilk/eframe_template/main/index.html), [main.rs](https://raw.githubusercontent.com/emilk/eframe_template/main/src/main.rs)).
- Renderer choice matters for bundle size: eframe defaults to wgpu, and "Switching from `wgpu (the default)` to `glow` can significantly reduce your binary size (including the .wasm of a web app)" ([docs.rs/eframe](https://docs.rs/eframe/0.37.0/eframe/)).
- Documented web limitations you must accept: "you cannot search an egui web page like you would a normal web page"; mobile text editing is weak; "Accessibility: There is an experimental screen reader … enabled explicitly" ([docs.rs/crate/eframe/0.37.0](https://docs.rs/crate/eframe/0.37.0) — "Limitations when running egui on the web"). eframe itself recommends the web target "for web apps where performance and responsiveness are more important than accessibility and mobile text editing."
- The rest of the app is normal Rust: eframe lists companion crates that work on both targets — `ehttp`/`reqwest` for HTTP, `ewebsock` for WebSockets, `chrono`, `rfd` ([docs.rs/crate/eframe/0.37.0](https://docs.rs/crate/eframe/0.37.0) — "Not all rust crates work when compiled to Wasm, but here are some useful crates…").

**Fit for this repo:** strong. The UI is panels, scroll areas, tables and text — egui's core vocabulary. The existing `App` state machine maps almost 1:1; only the render functions are rewritten.

---

## Option 3 — trunk

**Verdict: the dev server to use with egui.**

- What it is: "a WASM web application bundler for Rust … building & bundling WASM, JS snippets & other assets … via a source HTML file" ([trunkrs.dev](https://trunk-rs.github.io/trunk/)).
- `trunk build` "runs a cargo build targeting the wasm32 instruction set, runs `wasm-bindgen` … and spawns asset build pipelines"; `trunk watch` adds a filesystem watch; "`trunk serve` does the same thing as `trunk watch`, but also spawns a web server" ([Trunk guide — Commands](https://trunk-rs.github.io/trunk/guide/commands/)).
- Rebuild + browser refresh is explicit: "Trunk has built-in support for watching for source file changes, which triggers a rebuild and a refresh in the browser", and autoreload is a WebSocket (`ws_protocol = "ws"`, `no_autoreload = false`) ([Trunk guide — Configuration](https://trunk-rs.github.io/trunk/guide/configuration/)).
- eframe_template's documented loop is exactly this: install `wasm32-unknown-unknown`, install Trunk, "Run `trunk serve` to build and serve on `http://127.0.0.1:8080`. Trunk will rebuild automatically if you edit the project." ([README](https://raw.githubusercontent.com/emilk/eframe_template/main/README.md)).
- Minimal setup: one `index.html` with `<link data-trunk rel="rust"/>` ([app setup](https://trunk-rs.github.io/trunk/)), optional `Trunk.toml` with `[serve] port/addresses/no_autoreload` and `[[proxy]] backend/ws/rewrite` ([configuration](https://trunk-rs.github.io/trunk/guide/configuration/)).
- macOS arm64 install: `brew install trunk`, or `cargo install trunk --locked`, or a released binary ([README](https://raw.githubusercontent.com/trunk-rs/trunk/main/README.md)).
- Trunk also proxies, which is how the browser build reaches a local sidecar without CORS: `[[proxy]] backend = "https://localhost:9000/api/v1"` / `ws = false` ([configuration](https://trunk-rs.github.io/trunk/guide/configuration/)).
- It watches, it does not hot-swap: reload resets wasm memory, so in-flight debate state must live in the bridge, not the page.

Version observed: **trunk 0.22.0** ([crates.io](https://crates.io/crates/trunk/0.22.0)).

---

## Option 4 — wasm-server-runner + cargo-watch

**Verdict: viable fallback; less capable than trunk for an eframe app.**

- `wasm-server-runner` is "cargo run for wasm programs": you add a runner to `~/.cargo/config.toml` and then `cargo run --target wasm32-unknown-unknown` prints "starting webserver at http://127.0.0.1:1334" ([README](https://raw.githubusercontent.com/jakobhellermann/wasm-server-runner/main/README.md)).
- Its reload contract is weaker: "The website will reload when the server is restarted and serve files relative to the current directory" ([README](https://raw.githubusercontent.com/jakobhellermann/wasm-server-runner/main/README.md)). The runner itself does not watch files.
- Pairing it with cargo-watch supplies the watch: "Cargo Watch is a tool to watch your Cargo-based project and run commands when files change", e.g. `cargo watch -x 'run --target wasm32-unknown-unknown'` ([README](https://raw.githubusercontent.com/watchexec/cargo-watch/main/README.md)).
- Compared with trunk, you lose: asset pipeline (no `data-trunk` links, no wasm-opt/sass hooks), index.html templating, SRI, and the proxy section. You keep a single command and no extra `index.html` contract.
- For eframe specifically, the documented path is trunk (eframe docs point at eframe_template, which uses Trunk) — so this option is only worth it if you want `cargo run` symmetry and no trunk install.

Versions observed: **wasm-server-runner 1.0.1**, **cargo-watch 8.5.3**.

---

## Option 5 — hot-lib-reloader

**Verdict: a native-only enhancement, not a web answer.**

- Mechanism: "a development tool that allows you to reload functions of a running Rust program", built on `libloading`, requiring "code you want to hot-reload inside a Rust library (dylib)" ([docs.rs/hot-lib-reloader](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/)).
- Structure: a workspace with a `bin` crate and a `lib` crate; `lib` sets `crate-type = ["rlib", "dylib"]`; hot functions are `#[unsafe(no_mangle)]`; the bin wraps them with `#[hot_lib_reloader::hot_module(dylib = "lib")]` + `hot_functions_from_file!` ([docs.rs](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/) — "Example project setup").
- Run loop: two watchers, "cargo watch -w lib -x 'build -p lib'" alongside "cargo run", with the observer `wait_for_reload()` to drive redraws ([docs.rs](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/) — "Running it", "lib-reload events").
- macOS arm64: supported, with caveats — the library must be codesigned ("hot-lib-reloader will try to use the `codesign` binary that is part of the XCode command line tools"), the changelog records "Fix reloads on macOS M1" and "we will codesign the library when the `codesign` binary is available" ([docs.rs](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/) — "Prerequisites: macOS"; [CHANGELOG](https://raw.githubusercontent.com/rksm/hot-lib-reloader-rs/master/CHANGELOG.md)).
- Limitations that bite a UI: "No signature changes" (crash), "Type changes require some care" (UB/crash), "Hot-reloadable functions cannot be generic", and global state needs re-initialisation; `tracing` is a known hazard ([docs.rs](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/) — "Know the limitations", "Known issues").
- egui fit is explicitly supported: the docs list an "egui — How to hot-reload a native egui / eframe app" example, run with `cargo run --features reload` plus `cargo watch -w lib -x "build -p lib"` ([docs.rs](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/) — "Examples"; [hot-egui README](https://raw.githubusercontent.com/rksm/hot-lib-reloader-rs/master/examples/hot-egui/README.md)).
- Not usable for the browser target: it loads a platform dylib at runtime via `libloading`, and the crate documents no wasm path — the whole mechanism is the native dynamic loader, so a browser build is out of scope [INFERENCE].

Version observed: **hot-lib-reloader 0.8.2**.

---

## Option 6 — Dioxus

**Verdict: best-in-class reload, highest churn risk, and a WebView desktop target.**

- One codebase claim: "Build for web, desktop, and mobile, and more with a single codebase" ([README](https://raw.githubusercontent.com/DioxusLabs/dioxus/main/README.md)).
- Dev loop: `dx serve` "Build, watch, and serve the project" and `dx run` "Run the project without any hotreloading" ([Tools guide](https://dioxuslabs.com/learn/0.7/guides/tools/) — CLI help output, verbatim).
- Three reload tiers: element/RSX hot-reload (no full recompile), asset/CSS hot-reload, and Rust hot-patching. "Dioxus 0.7 introduces experimental Rust hot-reloading", enabled with `dx serve --hotpatch` under the **Subsecond** engine ([hot-reload reference](https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload/)).
- Stated hot-patch limits: globals' destructors never run, renamed globals are treated as new, static initializers are not observed, and "Rust hot-patching currently only tracks the 'tip' crate in your project" ([same page](https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload/) — "Experimental: Rust Hot-patching"). The fullstack guide repeats: "For the best experience, we recommend only modifying code in the 'tip' of your app" ([Fullstack](https://dioxuslabs.com/learn/0.7/essentials/fullstack/)).
- Subsecond is framework-independent: "Subsecond also works outside Dioxus … Bevy and Iced already have Subsecond integrations" ([hot-reload reference](https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload/)).
- Renderers: web renders "directly to the DOM using WebAssembly"; desktop renders "using Webview or - experimentally - with WGPU" ([README](https://raw.githubusercontent.com/DioxusLabs/dioxus/main/README.md)). So the native app hosts HTML/CSS in a WebView — visually web-grade — and the desktop binary is a native process, so it can spawn `uv` itself.
- Server side, if you ever want it: fullstack "deeply integrates with the popular Axum framework" and supports "Server Functions: Type-safe Axum HTTP endpoints directly callable from the client" ([Fullstack](https://dioxuslabs.com/learn/0.7/essentials/fullstack/)). A Rust server could own the `council.py` child process, but the engine is Python and the bridge is small either way (roughly 200 lines by inspection of the current `src/backend.rs` [estimate]).
- Release state on 2026-10-23: stable **dioxus 0.7.10 (2026-07-30)**; newest published **0.8.0-alpha.3 (2026-10-16)**; `dioxus-cli` latest published is **0.8.0-alpha.3**, while `dioxus-cli@0.7.10` also exists ([docs.rs/crate/dioxus](https://docs.rs/crate/dioxus/0.7.10); registry via `cargo info`). Pinning CLI to framework version is on you.
- UI fit: styled with HTML/CSS ("Dioxus apps are styled with HTML and CSS", [README](https://raw.githubusercontent.com/DioxusLabs/dioxus/main/README.md)). A dense council dashboard is a normal CSS layout problem — good ceiling, but you are writing markup, and the 0.8 alpha means API drift.

Versions observed: **dioxus 0.7.10** / 0.8.0-alpha.3, **dioxus-cli 0.8.0-alpha.3**, **subsecond 0.8.0-alpha.3**.

---

## Option 7 — Slint

**Verdict: excellent preview loop, wrong shape for this repo (DSL split + licence terms).**

- Model: "Write your UI once in `.slint`, a simple markup language. Connect it to business logic written in Rust, C++", compiled ahead of time to native code ([README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md)).
- Live preview (in-app) is the strongest feature here: it "lets your running application watch `.slint` files on disk and reload them automatically when they change", preserving properties/callbacks/models, enabled with `--features slint/live-preview` and `SLINT_LIVE_PREVIEW=1` ([docs.slint.dev](https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview/)). Caveat, quoted: "If you change the Slint API that the native code interacts with … the application may terminate."
- Standalone iteration: `slint-viewer --auto-reload path/to/myfile.slint` — "watch the file on disk and reload the preview whenever it changes" ([Slint Viewer docs](https://docs.slint.dev/latest/docs/slint/guide/tooling/slint-viewer/)); the same preview powers the VS Code extension and LSP ([README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md) — "A LSP Server that adds features like auto-complete and live preview").
- Web: "Web using WebAssembly" demos are shipped ([README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md)), and the winit backend "supports Windows, macOS, web browsers, X11 and Wayland" ([cargo features docs](https://docs.slint.dev/latest/docs/rust/slint/docs/cargo_features/)). A documented Rust→wasm→trunk recipe was **not found** in the primary docs — [unverified]. Slint also ships a JS `wasm-interpreter` package ([api/wasm-interpreter](https://github.com/slint-ui/slint/tree/master/api/wasm-interpreter)).
- Licensing: the crate is `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` ([crates.io/slint](https://crates.io/slint/1.18.1)). The royalty-free option permits proprietary "Desktop, Mobile, or Web Application" but **requires attribution** — either the `AboutSlint` widget in an About screen or the Slint badge on a public page — and excludes Embedded Systems ([license text](https://raw.githubusercontent.com/slint-ui/slint/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md), §2). GPLv3 is the alternative "Build open source applications" ([pricing](https://slint.dev/pricing)). For an MIT-licensed personal repo, the GPLv3 route would make the combined work GPLv3 [INFERENCE]; the royalty-free route keeps MIT but adds the attribution obligation. Paid plans exist but are unnecessary at this scale: the "Startup & Individual" plan is "Limited to individuals or companies with upto 10 employees" ([pricing](https://slint.dev/pricing)).
- UI fit: declarative layout with a std-widgets library; dense tables/charts are possible but you write a second language and cannot reuse `src/model.rs` rendering logic directly. Marked [INFERENCE] — no primary source measures dashboard density.

Version observed: **slint 1.18.1**.

---

## Option 8 — iced

**Verdict: web target exists but is under-documented; not the low-risk choice.**

- Positioning: "A cross-platform GUI library for Rust focused on simplicity and type-safety. Inspired by Elm"; features list "Cross-platform support (Windows, macOS, Linux, and the Web)" ([README](https://raw.githubusercontent.com/iced-rs/iced/master/README.md)).
- Web evidence in-repo: the examples say the Tour "can run both on native platforms and the web!"; the workspace pulls `wasm-bindgen-futures`, `wasmtimer`, `web-sys` ([examples README](https://raw.githubusercontent.com/iced-rs/iced/master/examples/README.md); [Cargo.toml](https://raw.githubusercontent.com/iced-rs/iced/master/Cargo.toml)).
- 0.14's changelog shows active wasm maintenance: fixes for "WebGPU failing to boot in Chromium", "Crash when using WebGL due to wrong binding alignment", "Panic on Chromium-like browsers when canvas initial size is (0, 0)", plus "animation module support for Wasm target" ([CHANGELOG](https://raw.githubusercontent.com/iced-rs/iced/master/CHANGELOG.md)).
- Rendering on web is canvas-based through wgpu: "Applications can now be rendered into an HTML `canvas` when targeting Wasm by leveraging the WebGL support in wgpu" (0.10 entry, [CHANGELOG](https://raw.githubusercontent.com/iced-rs/iced/master/CHANGELOG.md)).
- Maturity warning is the project's own: "Iced is currently experimental software" ([README](https://raw.githubusercontent.com/iced-rs/iced/master/README.md)).
- No official trunk/index.html/`wasm-bindgen` setup page was found in the book or README — [unverified]. Any wasm dev loop here would be assembled by hand.
- Hot reload: no first-party story; Subsecond advertises integrations for "Bevy and Iced" ([Dioxus hot-reload reference](https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload/)), which is third-party relative to iced.

Version observed: **iced 0.14.0**.

---

## Versions observed on 2026-10-23

| Tool / crate | Version | Note |
|---|---|---|
| ratatui | 0.30.2 | repo already on it (`Cargo.toml`) |
| ratzilla | 0.3.1 | community/ratatui-org web backend for ratatui |
| eframe | 0.37.0 | released 2026-10-15 ([docs.rs/crate/eframe](https://docs.rs/crate/eframe/0.37.0)) |
| egui | 0.37.0 | "runs on both web and native" ([crates.io](https://crates.io/crates/egui/0.37.0)) |
| egui_extras | 0.37.0 | tables, images, dates ([crates.io](https://crates.io/crates/egui_extras/0.37.0)) |
| egui_plot | 0.38.0 | 2026-10-19; depends on `egui ^0.37.0` ([docs.rs/crate](https://docs.rs/crate/egui_plot/0.38.0)) |
| egui_commonmark | 0.26.0 | Markdown viewer; egui-version compatibility [unverified] |
| eframe_template | egui 0.36.0 | lags the 0.37 release ([Cargo.toml](https://raw.githubusercontent.com/emilk/eframe_template/main/Cargo.toml)) |
| trunk | 0.22.0 | ([crates.io](https://crates.io/crates/trunk/0.22.0)) |
| wasm-server-runner | 1.0.1 | ([crates.io](https://crates.io/crates/wasm-server-runner/1.0.1)) |
| cargo-watch | 8.5.3 | ([crates.io](https://crates.io/crates/cargo-watch/8.5.3)) |
| hot-lib-reloader | 0.8.2 | ([docs.rs](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/)) |
| dioxus | 0.7.10 stable; 0.8.0-alpha.3 pre | 0.7.10 on 2026-07-30, alpha on 2026-10-16 ([docs.rs/crate](https://docs.rs/crate/dioxus/0.7.10)) |
| dioxus-cli (`dx`) | 0.8.0-alpha.3 latest; 0.7.10 available | registry via `cargo info` |
| subsecond | 0.8.0-alpha.3 | ([crates.io](https://crates.io/crates/subsecond/0.8.0-alpha.3)) |
| slint | 1.18.1 | triple-licensed ([crates.io](https://crates.io/crates/slint/1.18.1)) |
| iced | 0.14.0 | ([crates.io](https://crates.io/crates/iced/0.14.0)) |
| ehttp | 0.7.2 | released 2026-10-21 ([docs.rs/crate](https://docs.rs/crate/ehttp/0.7.2)) |
| gloo-net | 0.7.0 | browser `EventSource` wrapper ([docs.rs](https://docs.rs/gloo-net/latest/gloo_net/eventsource/index.html)) |
| axum | 0.8.9 | `response::sse` module ([docs.rs](https://docs.rs/axum/latest/axum/response/sse/index.html)) |
| tower-http | 0.7.1 | `ServeDir` for shipping `dist/` ([docs.rs](https://docs.rs/tower-http/latest/tower_http/services/struct.ServeDir.html)) |

---

## Comparison

| Option | Same code native + web | Rust-edit reload | Preserves state | Can drive `council.py` from browser | UI ceiling | Licence | Risk |
|---|---|---|---|---|---|---|---|
| Ratatui + Ratzilla | mostly (terminal cells) | trunk rebuild + page reload | no | no (needs bridge) | terminal grid only | MIT | low tech risk, **fails the "pretty" goal** |
| **egui/eframe + trunk** | **yes** (`App` + `WebRunner`) | **trunk: rebuild + reload** | no (page reload discards in-page state) | no directly; **yes via sidecar** | high (custom painting, fonts, tables, plots) | MIT/Apache-2.0 | low; newest egui 0.37 is 8 days old |
| egui native + hot-lib-reloader | native only | **hot swap, no restart** | **yes** | n/a (no browser) | high | MIT | medium (dylib split, codesign, no generics/signature changes) |
| Dioxus | yes (DOM in browser, WebView desktop) | `dx serve`; RSX hot-reload; `--hotpatch` experimental | RSX yes; Rust patch partial | via axum server functions | high (HTML/CSS) | MIT/Apache-2.0 | medium-high (0.8 alpha churn) |
| Slint | yes (wasm demos; winit web) | **live preview**, state preserved | **yes** | needs bridge | medium-high (DSL + widgets) | GPLv3 / royalty-free (attribution) / commercial | medium (DSL, licence obligation, web recipe undocumented) |
| iced | yes (canvas/wgpu) | none first-party | n/a | needs bridge | medium-high | MIT | high (self-described experimental; no wasm setup docs) |

Full-text sources for each cell are in the per-option sections above; the two "needs bridge" cells are consequences of the constraint section that follows.

---

## The wasm/process tension — verified, then three ways out

### The constraint

`wasm32-unknown-unknown` is "a WebAssembly compilation target which does not import any functions from the host for the standard library", and "many parts of the standard library do not work and return errors. For example `println!` does nothing, `std::fs` always return errors, and `std::thread::spawn` will panic. There is no means by which this can be overridden." ([rustc book](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)).

Consequences for this repo, in order of pain:

1. `std::process::Command` in `src/backend.rs` cannot exist in the wasm build at all — it is not "slow", it is not ported. The module must be `#[cfg(not(target_arch = "wasm32"))]`.
2. `scan_sessions("reports/")` (`src/app.rs:36-37`) cannot read the filesystem. The browser must ask a server for the session list.
3. Context-file attachment (`--context`) cannot read a picked file. The browser must upload it (or let the server read a path).
4. The browser page also has no OS-level `uv`/PATH environment, so even a bridge must carry the working directory and env explicitly.

### Workaround A — native sidecar bridge (recommended)

A small Rust server owns the child process and speaks HTTP + SSE to the wasm UI.

- Server side: axum with `axum::response::sse::{Event, KeepAlive, Sse}` — "Server-Sent Events (SSE) responses" ([docs.rs/axum](https://docs.rs/axum/latest/axum/response/sse/index.html)); spawning via `tokio::process::Command`, "an implementation of asynchronous process management for Tokio" ([docs.rs/tokio](https://docs.rs/tokio/latest/tokio/process/index.html)). The existing `src/backend.rs` line reader becomes a `tokio::io::BufReader::lines()` loop that maps each parsed `BackendEvent` to one SSE `data:` frame.
- Client side in wasm: `ehttp`, "Minimal HTTP client for both native and WASM" ([docs.rs/ehttp](https://docs.rs/ehttp/0.7.2/ehttp/)), with the `streaming` feature: "instead of receiving the response in full, you receive parts of the response as they are streamed in" ([ehttp::streaming](https://docs.rs/ehttp/0.7.2/ehttp/streaming/index.html)). Each `Part::Chunk` is fed to a line buffer; `data: {json}` lines deserialize into the same `BackendEvent` (`src/model.rs`).
- Same crate on native: ehttp uses `ureq` natively and `web-sys`/`wasm-bindgen` on wasm ([docs.rs/crate/ehttp/0.7.2](https://docs.rs/crate/ehttp/0.7.2) — dependency list). One client module, two transports underneath, chosen by the crate.
- If you prefer browser-native SSE semantics, `gloo-net::eventsource` is "Wrapper around the EventSource API" ([docs.rs/gloo-net](https://docs.rs/gloo-net/latest/gloo_net/eventsource/index.html)) — but it is browser-only, so it cannot serve the native build.
- Dev CORS: avoid it with Trunk's proxy — `[[proxy]] backend = "…" ws = false` ([Trunk configuration](https://trunk-rs.github.io/trunk/guide/configuration/)).
- Production: serve the wasm `dist/` from the same axum server with `tower_http::services::ServeDir` ("Service that serves files from a given directory and all its sub directories", [docs.rs/tower-http](https://docs.rs/tower-http/latest/tower_http/services/struct.ServeDir.html)), so the app is same-origin and the bridge disappears from the URL space.
- What eframe's docs recommend: there is no end-to-end "web + native + backend" guide; the documented pieces are the companion-client list ("HTTP client: ehttp and reqwest", "WebSockets: ewebsock") plus Trunk's proxy. Assembling them into a sidecar is the community pattern, and the ehttp demo itself is an eframe app ([ehttp README](https://raw.githubusercontent.com/emilk/ehttp/main/README.md) — "Uses eframe"). Treat the architecture as [INFERENCE] built from those documented parts.

### Workaround B — browser demo mode only

Build the wasm target for styling work: run the existing `--demo` path with canned turns (`src/app.rs` `new(demo_mode, …)`, `spawn_mock_council` in `src/backend.rs`) and ship real runs natively.

- Cost: near zero beyond the app itself; no bridge, no CORS, no SSE.
- Benefit: the fast visual loop the goal asks for — edit a layout, `trunk serve`, see it in the browser at 60fps.
- Limit: the browser cannot start a debate, open a session, or attach a file. That is acceptable for design iteration, not for "the UI".

### Workaround C — native only + hot-lib-reloader

Skip the browser; get state-preserving reload on the desktop binary.

- Requires the lib/bin split and `#[hot_module(dylib = "lib")]` plumbing, plus codesigning on macOS ([docs.rs/hot-lib-reloader](https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/)).
- Constraints are real: no signature changes, no generics, type layout changes are UB-ish, globals need reinit ("Know the limitations", same page).
- This is the right *complement* if you later want to iterate on a live debate without losing it; it is not a substitute for "served in a browser".

---

## Recommended architecture

**Shape:** one egui UI codebase; two entry points; one bridge; Trunk for the web dev loop.

```
council-ui (single package, lib + 2 bins)
├─ src/model.rs            shared serde types (already exists)          ── compiles to native + wasm
├─ src/claims.rs           shared claim extraction                      ── native + wasm
├─ src/app.rs              App state machine (tabs, selection, status)  ── native + wasm
├─ src/ui/*.rs             egui views (replaces ratatui draw fns)       ── native + wasm
├─ src/transport/
│   ├─ mod.rs              trait Transport { start, poll } -> BackendEvent
│   ├─ native.rs           spawn `uv run council.py` (current backend.rs)  [cfg(not(wasm32))]
│   └─ web.rs              ehttp streaming -> SSE line parser              [cfg(wasm32)]
├─ src/main.rs             native: eframe::run_native                    [cfg(not(wasm32))]
├─ src/lib.rs              wasm:  eframe::WebRunner + wasm-bindgen shim   [cfg(wasm32)]
└─ src/bin/council-bridge.rs  axum: POST /api/run, GET /api/events (SSE), GET /api/sessions,
                              POST /api/context, POST /api/cancel; serves dist/ in release
```

**Two dev loops, one UI:**

```bash
# native (real debates, direct spawn, no server)
cargo run -- --demo              # styling only, current --demo semantics
cargo run -- "<question>"        # real run through uv

# browser (same views; engine via bridge)
rustup target add wasm32-unknown-unknown
cargo install --locked trunk     # or: brew install trunk
cargo run --bin council-bridge   # 127.0.0.1:8787, spawns uv, emits SSE
trunk serve                      # 127.0.0.1:8080; `open = true` under [serve] opens a tab
```

**Why this is the smallest-risk option for *this* repo**

1. The engine seam does not move. `src/model.rs`'s `BackendEvent` is already the wire format; the bridge re-emits exactly those JSON objects, so there is no second protocol to keep in sync (unlike the current JS middleware).
2. Only the draw layer is rewritten. `src/app.rs` is 424 lines of state transitions and key handling; the view modules are 73–203 lines each. That is a bounded port.
3. Both transports produce the same `Vec<BackendEvent>` stream, so `App::handle_event` is target-independent.
4. Failure isolation: the bridge is a separate binary; a crash or a leaked `uv` child never takes the UI down, and `POST /api/cancel` maps to killing the child (`ProcessHolder` already exists in `src/backend.rs`).
5. No Node in the repo after cutover: `web/` (React 19, Vite, Tailwind) and its SSE middleware are deleted, so there is one build system (`cargo`) plus one dev server (`trunk`).

**Pin choice.** eframe/egui 0.37.0 shipped 2026-10-15 and eframe_template still pins 0.36.0 ([Cargo.toml](https://raw.githubusercontent.com/emilk/eframe_template/main/Cargo.toml)). Two defensible settings: pin **0.36.2 + egui_plot 0.37.0** (released 2026-08-05, alongside egui 0.36) to copy the template verbatim, or pin **0.37.0 + egui_plot 0.38.0** for the current API and adapt the ~40-line bootstrap. Prefer 0.37 if you want the newest `Modal`/`Theme` API; prefer 0.36 if you want the template's file layout to be copy-pasteable.

**Bundle and web caveats to plan for.** Use the `glow` renderer to shrink the wasm ("can significantly reduce your binary size", [docs.rs/eframe](https://docs.rs/eframe/0.37.0/eframe/)); accept that in-page text search and screen-reader support are weak ("you cannot search an egui web page", [docs.rs/crate/eframe](https://docs.rs/crate/eframe/0.37.0)); keep the browser build a *dev and demo* surface, with native as the primary app.

---

## Migration map (Ratatui → egui)

All egui names used below are from the 0.37.0 API docs: panels are `Panel::left/right/top/bottom` plus `CentralPanel` (egui 0.37 has no `SidePanel`/`TopBottomPanel` — both are 404 on docs.rs), `Modal` lives in the container module, and `ScrollArea`/`RichText`/`Theme` are top-level — see [docs.rs/egui/0.37.0](https://docs.rs/egui/0.37.0/egui/) (module `containers` — "Containers are pieces of the UI which wraps other pieces of UI. Examples: `Window`, `ScrollArea`, `Resize`, `Panel`") and the [panel module](https://docs.rs/egui/0.37.0/egui/containers/panel/index.html) ("Panels are `Ui` regions taking up e.g. the left side of a `Ui` or screen"); `Panel::left` is "Create a left panel" ([Panel](https://docs.rs/egui/0.37.0/egui/containers/panel/struct.Panel.html)).

| Today | File(s) | egui replacement |
|---|---|---|
| Root layout: header / body / footer, vertical+horizontal `Layout` | `src/ui/mod.rs` (`render`, `render_header`, `render_main_body`, `render_footer`) | `Panel::top("header")`, `Panel::bottom("footer")`, `Panel::left("sessions")`, `Panel::right("brief")`, `CentralPanel::default()` (add `CentralPanel` last — "Always add any `CentralPanel` last", [panel module](https://docs.rs/egui/0.37.0/egui/containers/panel/index.html)) |
| `Tabs` widget with numbered tabs | `src/ui/mod.rs`; keys in `src/app.rs:207-211` | `ui.horizontal(|ui| { for tab in Tab::ALL { ui.selectable_value(&mut self.active_tab, tab, tab.title()); } })` (keep the enum and `1..5` shortcuts via `ctx.input`) |
| Adversarial ladder as a `List` with status glyphs, model colours, duration/word counts | `src/ui/timeline.rs` | `ScrollArea::vertical()` + per-turn `Frame::group(ui.style())` or `Button::new(RichText…)` with `.selected(…)`; port `Theme::model_color` into an egui colour lookup |
| Turn card with wrapped text and scroll offset | `src/ui/turn_view.rs` | `ScrollArea::vertical().stick_to_bottom(true)` + `RichText` heading, `Label::new(...).wrap()`; progress spinner via `ui.spinner()` while `TurnStatus::Running` |
| Decision Brief: hand-styled Markdown-ish lines (`#`, `##`, `- `, `\|` tables), scrollbar | `src/ui/brief_view.rs` | `ScrollArea` + either port `format_brief_content` to `LayoutJob` runs, or use `egui_commonmark 0.26.0` ("Commonmark viewer for egui"); recommendation/confidence lines become coloured `RichText` |
| Claim ledger `Table` with selected row + inspection pane | `src/ui/claims.rs` | `egui_extras::TableBuilder::new(ui).column(Column::auto().resizable(true)).striped(true).sense(Sense::click())` with `header`/`body` (API verified in [egui_extras 0.37.0](https://docs.rs/egui_extras/0.37.0/egui_extras/struct.TableBuilder.html)); inspection pane = bottom `Panel` or `ScrollArea` |
| Sessions list + preview, scanning `reports/` | `src/ui/sessions.rs`, `src/session_loader.rs` | Left panel list + right preview; **data source diverges by target**: native keeps `scan_sessions`, wasm calls `GET /api/sessions` |
| Composer modal with three fields and `Tab` cycling | `src/ui/composer.rs`, composer branch in `src/app.rs` | `egui::Modal` ([`Modal` in egui 0.37](https://docs.rs/egui/0.37.0/egui/)) with `TextEdit::singleline`/`multiline`, `DragValue` for rounds; file attach = native `rfd` dialog, web `POST /api/context` |
| Help overlay | `src/ui/help.rs` | `egui::Window::new("Help").open(&mut open)` with `Grid` two-column key list |
| Theme struct of 15 colours + `Style` helpers | `src/ui/theme.rs` | Keep the palette; map to `Visuals` (`window_fill`, `widgets.noninteractive.fg_stroke`, `selection.bg_fill`, etc. — "Controls the visual style (colors etc) of egui") and `Context::set_fonts` for the font stack ("If you want to change fonts, use `Context::set_fonts`") ([docs.rs/egui Visuals](https://docs.rs/egui/0.37.0/egui/style/struct.Visuals.html)) |
| `Theme` enum / dark-light switch | — | `egui::Theme::{Dark, Light}` ("Dark or Light theme") + `ThemePreference` ([docs.rs/egui](https://docs.rs/egui/0.37.0/egui/)) |
| Key handling (`KeyCode::Char('j')`, `PageUp`, `Tab`) | `src/app.rs` (`handle_key`) | `ctx.input(\|i\| i.key_pressed(Key::J))`; arrow/scroll handling mostly free via `ScrollArea` |
| Optional charting (none today; likely next) | — | `egui_plot 0.38.0` — "Immediate mode plotting for the egui GUI library", depends on `egui ^0.37.0` and its own demo is served by `trunk serve` ([docs.rs/crate/egui_plot](https://docs.rs/crate/egui_plot/0.38.0)) |

**Dependency changes:** drop `ratatui`, `crossterm`, `tui-textarea`; add `eframe`/`egui` (+ `egui_extras` for the claim table), `ehttp` with `streaming`, `axum` + `tokio` + `tower-http` for the bridge, and `wasm-bindgen-futures`/`web-sys` behind `cfg(target_arch = "wasm32")`. `serde`, `serde_json`, `clap`, `chrono` stay.

**What must not change:** `src/model.rs` (wire format), the `uv run council.py --json-stream` argument order (`build_command` in `src/backend.rs`), the `reports/` layout, and the demo path. Those are the contract the bridge and both UIs share.

---

## Open questions

1. **egui 0.37 vs 0.36.** The template lags one release; picking 0.37 means adapting the web bootstrap by hand. Not resolved by the docs — a one-hour spike decides it.
2. **SSE framing details.** ehttp streaming yields raw chunks, not parsed events; the exact chunk boundaries are unspecified in the docs, so the client needs a buffered line parser. Test against a real `council.py` run before committing to the design.
3. **Cancel semantics through the bridge.** `tokio::process` "will, by default, continue to execute even after the `Child` handle has been dropped" ([docs.rs/tokio](https://docs.rs/tokio/latest/tokio/process/index.html)), so cancellation must be explicit (`kill_on_drop` or a kill endpoint). The current TUI already marks runs cancelled without aborting the HTTP call (`src/app.rs`), so behaviour would match, not improve.
4. **egui_commonmark 0.26 ↔ egui 0.37 compatibility** — [unverified]; if it lags, port `format_brief_content` instead.
5. **Trunk autoreload on compile failure.** `no_error_reporting` exists in config, but the exact browser behaviour on a failed rebuild was not verified; check before relying on it as the primary loop.
6. **Wasm bundle size after `glow`.** The docs promise a reduction but give no number; measure once the views exist.
7. **Slint's Rust→wasm recipe** and **iced's wasm setup** are both undocumented in primary sources — [unverified]. If either becomes a finalist, budget a spike.
8. **Dioxus 0.8.** If HTML/CSS fidelity ends up mattering more than immediate-mode control, re-evaluate after 0.8 ships stable; today the CLI's latest publish is an alpha ([docs.rs/crate/dioxus](https://docs.rs/crate/dioxus/0.7.10)), which is a poor base for a repo that must keep working.
9. **Whether browser is a product surface or a dev convenience.** If it is only a dev convenience, Workaround B (demo-only wasm) removes the bridge entirely and the recommendation collapses to "egui + trunk + demo data".

---

## Sources

Primary pages used, all accessed 2026-10-23:

- Ratatui: <https://ratatui.rs/concepts/backends/>, <https://ratatui.rs/installation/>, <https://docs.rs/ratatui/0.30.2/ratatui/backend/index.html>, <https://docs.rs/ratatui/0.30.2/ratatui/backend/trait.Backend.html>
- Ratzilla: <https://docs.rs/ratzilla/0.3.1/ratzilla/>
- eframe/egui: <https://docs.rs/eframe/0.37.0/eframe/>, <https://docs.rs/crate/eframe/0.37.0>, <https://docs.rs/egui/0.37.0/egui/>, <https://docs.rs/egui/0.37.0/egui/style/struct.Visuals.html>, <https://crates.io/crates/egui/0.37.0>
- eframe_template: <https://raw.githubusercontent.com/emilk/eframe_template/main/README.md>, <https://raw.githubusercontent.com/emilk/eframe_template/main/Cargo.toml>, <https://raw.githubusercontent.com/emilk/eframe_template/main/index.html>, <https://raw.githubusercontent.com/emilk/eframe_template/main/src/main.rs>
- Trunk: <https://trunk-rs.github.io/trunk/>, <https://trunk-rs.github.io/trunk/guide/commands/>, <https://trunk-rs.github.io/trunk/guide/configuration/>, <https://raw.githubusercontent.com/trunk-rs/trunk/main/README.md>, <https://crates.io/crates/trunk/0.22.0>
- wasm-server-runner / cargo-watch: <https://raw.githubusercontent.com/jakobhellermann/wasm-server-runner/main/README.md>, <https://raw.githubusercontent.com/watchexec/cargo-watch/main/README.md>, <https://crates.io/crates/wasm-server-runner/1.0.1>, <https://crates.io/crates/cargo-watch/8.5.3>
- hot-lib-reloader: <https://docs.rs/hot-lib-reloader/0.8.2/hot_lib_reloader/>, <https://raw.githubusercontent.com/rksm/hot-lib-reloader-rs/master/CHANGELOG.md>, <https://raw.githubusercontent.com/rksm/hot-lib-reloader-rs/master/examples/hot-egui/README.md>
- Dioxus: <https://raw.githubusercontent.com/DioxusLabs/dioxus/main/README.md>, <https://dioxuslabs.com/learn/0.7/guides/tools/>, <https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload/>, <https://dioxuslabs.com/learn/0.7/essentials/fullstack/>, <https://docs.rs/crate/dioxus/0.7.10>, <https://crates.io/crates/subsecond/0.8.0-alpha.3>
- Slint: <https://raw.githubusercontent.com/slint-ui/slint/master/README.md>, <https://docs.slint.dev/latest/docs/slint/guide/tooling/slint-viewer/>, <https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview/>, <https://docs.slint.dev/latest/docs/rust/slint/docs/cargo_features/>, <https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/>, <https://crates.io/crates/slint/1.18.1>, <https://slint.dev/pricing>, <https://raw.githubusercontent.com/slint-ui/slint/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md>
- iced: <https://raw.githubusercontent.com/iced-rs/iced/master/README.md>, <https://raw.githubusercontent.com/iced-rs/iced/master/examples/README.md>, <https://raw.githubusercontent.com/iced-rs/iced/master/Cargo.toml>, <https://raw.githubusercontent.com/iced-rs/iced/master/CHANGELOG.md>, <https://book.iced.rs/>, <https://crates.io/crates/iced/0.14.0>
- Wasm constraint: <https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html>
- Transport: <https://docs.rs/ehttp/0.7.2/ehttp/>, <https://docs.rs/ehttp/0.7.2/ehttp/streaming/index.html>, <https://docs.rs/crate/ehttp/0.7.2>, <https://raw.githubusercontent.com/emilk/ehttp/main/README.md>, <https://docs.rs/gloo-net/latest/gloo_net/eventsource/index.html>, <https://docs.rs/axum/latest/axum/response/sse/index.html>, <https://docs.rs/tokio/latest/tokio/process/index.html>, <https://docs.rs/tower-http/latest/tower_http/services/struct.ServeDir.html>
- Plotting: <https://docs.rs/crate/egui_plot/0.38.0>, <https://crates.io/crates/egui_extras/0.37.0>, <https://crates.io/crates/egui_commonmark/0.26.0>
- Repo: `Cargo.toml`, `src/model.rs`, `src/app.rs`, `src/backend.rs`, `src/ui/*.rs`, `web/vite.config.ts`
