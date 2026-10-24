# Adversarial Council

Ask a decision question once. It runs a red-team chain across four AI providers and gives you a one-page executive decision brief.

```
Mistral (opening position)
   → OpenRouter   attacks Mistral's arguments
   → Ollama Cloud attacks OpenRouter's critique
   → OpenCode Go  attacks Ollama Cloud's critique
   → Mistral      concedes / rebuts, revises its recommendation
   → Mistral      writes the Decision Brief (with the full debate attached as an appendix)
```

Each model gets the whole debate so far, but its job is to attack the **most recent** output. Every claim is numbered (C1, R1…) so later models can target specific points.

## Setup (one time, ~2 minutes)

1. Install [uv](https://docs.astral.sh/uv/getting-started/installation/) (it also manages the Python version — 3.14 is pinned in `.python-version`, no separate Python install needed):
   ```
   curl -LsSf https://astral.sh/uv/install.sh | sh   # macOS/Linux
   ```
2. In this folder, run:
   ```
   uv sync --all-extras
   ```
   (`--all-extras` adds optional readers for PDF/DOCX/XLSX context files; skip it if you only need txt/md/csv.)
3. Copy `.env.example` to `.env` and paste your API keys (or export them in `~/.zshenv`):
   - Mistral: `MISTRAL_API_KEY`
   - OpenRouter: `OPENROUTER_API_KEY`
   - Ollama Cloud: `OLLAMA_CLOUD_API_KEY`
   - OpenCode Go: `OPENCODE_API_KEY`
4. Check which models your keys can use, and set the ones you want in `.env`:
   ```
   uv run council.py --list-models
   ```

## Interactive UI (`council-ui`)

One Rust GUI built with [egui](https://www.egui.rs/) / [eframe](https://docs.rs/eframe) (`0.37`). It runs natively as a desktop app **and** in the browser as WebAssembly — the same views, the same engine seam (`uv run council.py --json-stream`).

### Quick start

```bash
# Instant offline demo (zero API keys needed)
cargo run -- --demo

# Dry run with Python backend (no API cost)
cargo run -- --dry-run "Should we expand our engineering hub to Nairobi or London?"

# Live debate with the four AI providers
cargo run -- "Hire 3 engineers in Lagos or outsource to an agency?"
```

### Live browser editing (`trunk serve`)

Edit the Rust views in `src/ui/` and watch the browser refresh automatically:

```bash
rustup target add wasm32-unknown-unknown   # one time
cargo install --locked trunk               # one time (or: brew install trunk)

cargo run --bin council-bridge             # :8787, spawns council.py, streams SSE
trunk serve                                # :8080, rebuilds on save, reloads the tab
```

The wasm build cannot spawn processes, so the browser UI talks to the `council-bridge` sidecar (`POST /api/run` with an SSE stream, `GET /api/sessions`, `GET /api/session?path=…`, `POST /api/cancel`); trunk proxies `/api/*` so no CORS is needed in dev. Session files and context attachments are served by the bridge. The native app spawns `council.py` directly and needs no server.

### Decision workspace

The desktop and browser use the same Rust views. The browser is a live-reload preview for fast UI iteration.

- **Reading first:** session sidebar, persistent decision question, a Markdown reader with tables and inline formatting, and a collapsible debate panel.
- **Your council:** every role has its own provider and searchable model dropdown: opening analyst, three critiques, rebuttal, and decision brief. Reuse a provider or model in any combination. Catalogs come from your configured provider accounts; an editable model ID supports models omitted from a catalog. A listed model still needs to support the provider's chat-completions API.
- **Composer:** multiline questions, context attachments, one to five rounds, and an offline dry-run option. The call count is shown before launch. UI runs use one request attempt per stage, with automatic SDK retries disabled.
- **Live progress:** elapsed stage time and completed-stage count. Follow live is optional; reading an earlier stage pauses following. A completed brief is announced without changing your view.
- **Recovery:** errors stop the spinner and preserve completed arguments. Retry continues at the failed stage with the original role assignments. Restart runs the entire council again.
- **History:** stage checkpoints and chosen models are saved locally, including interrupted debates. Open a saved decision from the sidebar. New runs retain the entire multiline question.
- **Claim provenance:** prompts distinguish source material, assumptions, and judgement. A supplied claim needs an exact excerpt from your question or context; unmatched claims are downgraded to assumptions. An excerpt match verifies the quotation exists in the input, not the truth of the conclusion. Unchallenged claims are explicitly labelled **UNCHALLENGED**.

Use `n` for a new decision, `1`–`5` for views, `j`/`k` or arrow keys to navigate, `c` to stop, and `r` to rerun. All primary actions also have buttons. Native `q` quits.

The bridge accepts the same per-role configuration as native runs. `RunConfig.roles` maps `opening`, `critique_1`, `critique_2`, `critique_3`, `rebuttal`, and `brief` to `{ "provider": "Mistral", "model": "mistral-small-latest" }`. CLI users can supply this map with `--roles-json`; existing `--providers` / `--skip` behavior is retained. `--models-json` exposes catalog metadata without credentials.

Browser context attachments are stored in the ignored `.council-context/` directory (10 MB per file). Native attachments reference the selected local files. API keys remain in the Python backend environment.


## Development

Toolchain, all configured in `pyproject.toml` unless noted:

| Tool | Role | Limit |
|---|---|---|
| [uv](https://docs.astral.sh/uv/) | env + deps | `uv sync --all-extras` |
| [Ruff](https://docs.astral.sh/ruff/) | lint + format | config in `pyproject.toml` |
| [ty](https://docs.astral.sh/ty/) | type check | the whole repo |
| [pytest](https://docs.pytest.org/) | behaviour | `tests/` |
| [CodeLimit](https://getcodelimit.github.io/) | function length | ≤15 code lines |
| [lizard](https://github.com/terryyin/lizard) | CCN, params | ≤10, ≤5 |
| [radon](https://radon.readthedocs.io/) + [xenon](https://xenon.readthedocs.io/) | complexity report | advisory |
| [import-linter](https://import-linter.readthedocs.io/) | dependency contracts | tooling must not import the app |
| [Rust / Cargo](https://doc.rust-lang.org/cargo/) | unified egui UI + bridge | `cargo fmt`, `cargo clippy`, `cargo test` |

```
uv sync --all-extras                          # install dependencies
uv run pre-commit install                     # run the gates before each commit
uv run pre-commit run --all-files             # every gate, exactly as CI runs it
uv run pre-commit run codelimit-strict --all-files   # one gate; ids are in the hook config
uv run council.py --dry-run "test"            # smoke test, no API calls
```

The hook entries in `.pre-commit-config.yaml` are authoritative for flags and thresholds; the table above summarises the limits. CI runs the same hooks, so a green local run is a green build.

**Length policy — one number: ≤15 lines of code per function.** CodeLimit owns it; `scripts/check_function_length.py` enforces it using CodeLimit's own measurement algorithm (pass an argument to inspect a different threshold). lizard deliberately does not gate length: its "length" counts physical span including signature and decorators, so a second number would contradict the first. It gates cyclomatic complexity (≤10) and parameter count (≤5).

**Complexity is advisory.** radon is a reporter and always exits 0; xenon turns its ranks into an exit code, but that step is `continue-on-error` in CI. A function drifting from B to C is a signal to look, not a blocked merge.

**Why the import-linter contract points backwards:** the app is a top-level module (`council.py`), and import-linter can only root a graph at a package, so the expressible contract is "tooling must not import the app" rather than the reverse. If `council.py` is ever split into a package, invert it.

## Use

```
uv run council.py "Should we open a Nairobi office in Q2 2027 or serve East Africa remotely?"
```

Give the models your numbers and documents (text, Markdown, or CSV):

```
uv run council.py --file decision.md --context pnl_2026.csv --context market_notes.md
```

Run more attack loops for a high-stakes call:

```
uv run council.py --rounds 2 "..."
```

Test it without spending anything on API calls:

```
uv run council.py --dry-run "test"
```

The report is saved to `./reports/<date>_<question>.md`. The brief comes first, then the full debate.

## What's in the brief

Recommendation and confidence level, the options compared, the arguments that held up against attack, the ones that didn't, where the models still disagree, risks with early-warning signals, what to find out before you decide, and next steps for the first 30 days.

## Getting better results

- **Write the question as a decision**, with a deadline and limits: "Hire 3 engineers in Lagos or outsource to an agency, budget ₦X, decide by Dec 1".
- **Give it your real data** with `--context`. If you don't, the models fall back on generic assumptions.
- **Pay most attention to "Unresolved disagreements".** When multiple different models still disagree after the debate, that is usually where your own judgement or more research is needed.
- **Cost:** one round is 6 model calls, usually a few cents depending on the models and how much context you include.
- **Change the order or roles:** edit `run_council()` in `council.py`. The prompts are plain text at the top of the file.

*This is decision support, not a decision. Check facts and figures before acting.*

### Output policy and diagnostics

Council automatically uses the selected model's advertised output/context limits.
There is no 4,000-token application cap or token slider. Supported thinking models
use their lowest positive advertised reasoning level; unknown reasoning policies stop
before inference. Unknown output maxima retain provider defaults, labeled in diagnostics.
Truncated or reasoning-only responses are saved as incomplete and excluded from later
arguments. There are no automatic retries, continuations or summarizer calls.

Open **Diagnostics** (shortcut **6**) to inspect or copy request policy, token usage,
finish reasons, request IDs and local event history. Private request snapshots and
transport lifecycle records support later investigation. Cost and provider-side
cancellation remain subject to what each provider reports.
See [the audit, retention and investigation guide](docs/agents/diagnostics.md) and
[the comparative research](docs/research/2026-10-23-token-budgets-and-reasoning.md).
