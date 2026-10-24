# Development

[Documentation](README.md) · [Quality gate conventions](agents/quality-gates.md)

## Architecture

Council has a Python decision engine and one shared Rust GUI, built with egui/eframe 0.37. The same Rust views run natively and as WebAssembly. The browser exists as a live-reload preview for rapid UI iteration; desktop and browser should retain UI/behavior parity.

| Area | Location |
| --- | --- |
| Decision pipeline, prompts, CLI, document readers | `council.py` |
| Capability discovery, output/context/reasoning policy | `council_models.py` |
| LLM requests and response validation | `council_inference.py` |
| Request and event persistence | `council_diagnostics.py` |
| Application state and events | `src/app.rs`, `src/model.rs` |
| Shared UI views and theme | `src/ui/` |
| Native, browser, and offline transports | `src/transport/` |
| HTTP/SSE browser bridge | `src/bin/council-bridge.rs` |

The native app spawns `uv run council.py --json-stream` directly. WebAssembly cannot spawn processes, so its transport uses the bridge. API keys remain in the Python backend environment.

## Local desktop

Install Rust and uv, then run from the repository root:

```bash
uv sync --all-extras
cargo run -- --demo
```

`--demo` uses the Rust mock transport and sample responses. To exercise Python without inference:

```bash
cargo run -- --dry-run "Should we expand our engineering team?"
```

To open the live composer:

```bash
cargo run
```

Passing a question starts a run immediately, so use `--dry-run` when testing without a live-call allowance:

```bash
cargo run -- "Should we expand our engineering team?"
```

On macOS, `Start Council.command` starts the offline demo by default and passes any supplied arguments to the executable.

## Browser live reload

One-time setup:

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Start the bridge in one terminal, from the repository root:

```bash
cargo run --bin council-bridge
```

Start Trunk in a second terminal:

```bash
trunk serve
```

Open `http://127.0.0.1:8080`. Edit the Rust views in `src/ui/`; Trunk rebuilds and reloads the browser. The bridge listens on port 8787, and Trunk proxies `/api/*` to it, keeping browser requests same-origin. The native app needs neither server.

| Endpoint | Purpose |
| --- | --- |
| `POST /api/run` | Start a run and receive SSE events |
| `POST /api/cancel` | Cancel the active backend process |
| `GET /api/sessions` | List saved sessions |
| `GET /api/session?path=…` | Load a saved session |
| `POST /api/context?name=…` | Store a context attachment |

Browser attachments are stored in ignored `.council-context/` files with a 10 MB limit per upload. The bridge serves session files from the project's `reports/` directory. Use the default reports directory when checking browser history parity.

`RunConfig.roles` maps `opening`, `critique_1`, `critique_2`, `critique_3`, `rebuttal`, and `brief` to a provider/model object. Native and browser transports pass the same assignments to the engine. See [CLI integration options](cli.md#integration-and-troubleshooting) and [per-role configuration](cli.md#select-providers-or-assign-each-role).

## Quality checks

Install hooks once:

```bash
uv run pre-commit install
```

Run all required checks before calling work complete:

```bash
uv run pre-commit run --all-files
```

During iteration, run the hook owning the failure:

```bash
uv run pre-commit run codelimit-strict --all-files
```

| Tool | Role |
| --- | --- |
| uv | Python environment, dependencies, and pinned Python version |
| Ruff | Linting and formatting |
| ty | Python type checking |
| pytest | Python behavior and inference policy tests |
| CodeLimit | Maximum 15 code lines per Python function |
| lizard | Cyclomatic complexity ≤10 and parameters ≤5 |
| import-linter | Repository tooling must not import the app |
| Cargo | Rust formatting, Clippy, and tests |
| radon and xenon | Advisory complexity reports in CI |

The hook entries in `.pre-commit-config.yaml` own flags and thresholds. CI runs the same hooks; environment or platform failures can still differ from local results. Do not weaken a gate to make a change pass. See [quality gate conventions](agents/quality-gates.md) for ownership and the exception policy.

CodeLimit's code-line measurement is authoritative. lizard's physical function span includes signatures and decorators, so it gates complexity and parameters rather than imposing a second length limit. radon reports complexity; xenon is advisory with `continue-on-error` in CI.

The import-linter contract points from tooling to the top-level app module because it roots graphs at packages. If `council.py` becomes a package, revisit the direction of that contract.

Use mocks and saved-session replay for routine UI and policy checks. Bound live-call tests explicitly; a model catalog fetch is not an inference call, but live debates spend provider resources. The [diagnostics guide](agents/diagnostics.md) describes evidence to retain.

## Troubleshooting setup

- **`cargo` or `uv` is not found:** restart the terminal after installing the tool and confirm it is on `PATH`.
- **The first Rust build fails to link:** install the compiler/system prerequisites for your OS from the Rust installation guide. Linux desktop builds can need additional distribution packages for windowing and file dialogs; Windows may need the MSVC build tools.
- **A provider has no available models:** check its key and account access, then refresh models. A chat subscription alone may not provide API access.
- **The browser cannot start or read sessions:** confirm both terminals are running, ports 8080/8787 are free, and the bridge was started from the repository root.

Further design notes: [Rust UI/live reload research](research/2026-10-23-rust-ui-live-reload.md) and [token/reasoning research](research/2026-10-23-token-budgets-and-reasoning.md).
