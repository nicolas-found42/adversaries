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

The backend loads `.env` beside `council.py`. Existing environment variables take precedence. It also reads simple `export KEY=value` lines from `~/.zshenv` for variables not already set; quoted values keep their content, an unquoted ` #` starts a comment, and any other shell syntax is ignored rather than guessed at. `OLLAMA_API_KEY` and `OPENCODE_GO_API_KEY` are accepted as key aliases.

Keep `.env` private; it is ignored by Git. On POSIX the backend warns when `.env` is group- or world-readable, and `Start Council.bat` restricts the file to the current Windows user. Keys never enter generated files: diagnostics redact the value of any environment variable whose name ends in a credential word (`_API_KEY`, `_TOKEN`, `_SECRET`, `_PASSWORD`), on top of bearer values, and every file the app writes is created owner-only (`0600`) inside an owner-only directory (`0700`). Restart the app after changing environment settings and refresh model catalogs. API keys stay in the Python backend environment, including when using the browser preview.

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

Every generated file is written mode `0600`, and directories the app creates are `0700`; the HTML copy is additionally limited by a `default-src 'none'` content-security policy, so model-authored markup cannot run script or load a remote resource. A directory that already exists keeps its own mode, so an older `reports/` stays as it was. Treat the reports directory as sensitive either way.

The CLI's `--out` changes its output directory. Export `COUNCIL_REPORTS_DIR` before launching the native app to change both the backend's default output and native history location; use an absolute path for predictable behavior. Setting it only in `.env` changes Python output but does not set the Rust process environment. The browser bridge's history endpoints currently scan the project's `reports/` directory, so keep the default location for browser preview sessions.

## Tokens, reasoning, and cost

Council discovers capabilities for the exact selected provider/model. It uses advertised output/context limits where available, rather than a universal 4,000-token cap or a user-facing token slider. Unknown output maxima retain provider defaults, labeled as unknown. This does not mean unlimited output.

Supported thinking models use their lowest supported positive reasoning setting; unknown reasoning policies stop before inference. A low requested effort can still produce substantial reasoning tokens. Providers may not confirm the applied setting or report cost.

Incomplete or reasoning-only responses are saved but excluded from later arguments. There are no automatic retries, continuation calls, summaries, or fallback models. A manual retry or restart can incur new charges.

See [diagnostics and policy details](agents/diagnostics.md) and [the comparative research](research/2026-10-23-token-budgets-and-reasoning.md).

## Privacy

The workspace and reports are local. Live inference sends the question, extracted attachments, and accumulated debate to the selected providers. Check their terms before using confidential material.

Request snapshots contain the actual supplied text and can be sensitive. The app does not deliberately store API headers or raw reasoning traces. Retention is manual; see [recorded files and retention](agents/diagnostics.md#files-available-now) before sharing a report or diagnostics archive.
