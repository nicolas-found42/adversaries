# Provider setup and configuration

[Documentation](README.md) · [First live run](../README.md#run-your-first-real-decision)

## Set up API keys

Copy `.env.example` from the repository root to `.env`, then enter the keys for the providers you plan to use. Keys come from your provider account. A provider subscription or chat app login does not necessarily include API access or model entitlement.

| Provider in Council | Environment variable |
| --- | --- |
| Mistral | `MISTRAL_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |
| Ollama-Cloud | `OLLAMA_CLOUD_API_KEY` |
| OpenCode-Go | `OPENCODE_API_KEY` |

Unused providers can stay blank. With one account, select that provider for every role in the composer. The default role assignments span all four providers, so change those selections before starting if you only configured one.

The backend loads `.env` beside `council.py`. Existing environment variables take precedence. It also reads simple `export KEY=value` lines from `~/.zshenv` for variables not already set. `OLLAMA_API_KEY` and `OPENCODE_GO_API_KEY` are accepted as key aliases.

Keep `.env` private; it is ignored by Git. Restart the app after changing environment settings and refresh model catalogs. API keys stay in the Python backend environment, including when using the browser preview.

## Choose models

The UI's provider and model dropdowns are the usual way to assign roles. An editable model ID supports IDs missing from a catalog, but it does not guarantee that a model is compatible or available to your account.

These optional environment variables set provider defaults at startup:

| Variable | Built-in default |
| --- | --- |
| `MISTRAL_MODEL` | `mistral-small-latest` |
| `OPENROUTER_MODEL` | `meta-llama/llama-3.3-70b-instruct` |
| `OLLAMA_CLOUD_MODEL` | `deepseek-v4.1-flash` |
| `OPENCODE_GO_MODEL` | `deepseek-v4.1-flash` |

`OPENCODE_MODEL` is a fallback alias for the OpenCode Go model default. These are defaults in the code, not a promise of current provider availability. Check your catalog:

```bash
uv run council.py --list-models
```

`--models-json` returns catalog metadata for programmatic callers without returning credentials. Catalog retrieval can contact configured providers but does not generate a debate.

## Documents and output

Install optional readers with `uv sync --all-extras` for PDF, DOCX, and XLSX. Text, Markdown, and CSV use the base installation. The reader truncates extracted document text after 60,000 characters and marks the truncation in the assembled request snapshot. Do not assume a long attachment reached the model in full.

Native attachments reference selected local files. The browser uploads attachments to `.council-context/`, with a 10 MB maximum per file. That directory is ignored by Git.

Reports are stored in `reports/` by default:

- Markdown report: the decision brief followed by the debate.
- JSON checkpoint: session content, chosen roles, state, and diagnostics.
- Event journal and request snapshots: evidence for troubleshooting.

The CLI's `--out` changes its output directory. Export `COUNCIL_REPORTS_DIR` before launching the native app to change both the backend's default output and native history location; use an absolute path for predictable behavior. Setting it only in `.env` changes Python output but does not set the Rust process environment. The browser bridge's history endpoints currently scan the project's `reports/` directory, so keep the default location for browser preview sessions.

## Tokens, reasoning, and cost

Council discovers capabilities for the exact selected provider/model. It uses advertised output/context limits where available, rather than a universal 4,000-token cap or a user-facing token slider. Unknown output maxima retain provider defaults, labeled as unknown. This does not mean unlimited output.

Supported thinking models use their lowest supported positive reasoning setting; unknown reasoning policies stop before inference. A low requested effort can still produce substantial reasoning tokens. Providers may not confirm the applied setting or report cost.

Incomplete or reasoning-only responses are saved but excluded from later arguments. There are no automatic retries, continuation calls, summaries, or fallback models. A manual retry or restart can incur new charges.

See [diagnostics and policy details](agents/diagnostics.md) and [the comparative research](research/2026-10-23-token-budgets-and-reasoning.md).

## Privacy

The workspace and reports are local. Live inference sends the question, extracted attachments, and accumulated debate to the selected providers. Check their terms before using confidential material.

Request snapshots contain the actual supplied text and can be sensitive. The app does not deliberately store API headers or raw reasoning traces. Retention is manual; see [recorded files and retention](agents/diagnostics.md#files-available-now) before sharing a report or diagnostics archive.
