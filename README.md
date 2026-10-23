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
