# Command-line guide

[Documentation](README.md) · [Provider configuration](configuration.md)

The CLI runs the Python decision engine without the desktop UI. Install dependencies with `uv sync --all-extras` and configure the providers you select in `.env`.

## Start with a dry run

```bash
uv run council.py --dry-run "Should we launch on web first or build native apps?"
```

This uses fake responses and does not call an LLM. For a live run using the default provider chain:

```bash
uv run council.py "Should we launch on web first or build native apps?"
```

The default chain uses all four providers: Mistral opens, OpenRouter critiques, Ollama Cloud critiques, OpenCode Go critiques, then Mistral rebuts and writes the brief. Each model receives the debate so far; each critique targets the most recent output. Configure all four provider keys for this default flow, or select providers explicitly.

## Supply a question and context files

```bash
uv run council.py --file decision.md --context budget.csv --context market-notes.md
```

`--context` is repeatable. Text, Markdown, and CSV work with the base install; PDF, DOCX, and XLSX need the optional readers installed by `--all-extras`. See [document limits](configuration.md#documents-and-output).

## Select providers or assign each role

Use a subset of providers with the legacy chain:

```bash
uv run council.py --providers mistral,openrouter "Should we expand the team?"
```

`--skip` removes a provider and is repeatable. A shorter provider chain changes the number of critique stages and calls; it does not retain the UI's fixed three-critic structure.

To match the UI's six roles, pass a complete JSON map with `--roles-json`. For example, use one configured provider/model for every role:

```bash
uv run council.py --roles-json '{"opening":{"provider":"OpenCode-Go","model":"deepseek-v4.1-flash"},"critique_1":{"provider":"OpenCode-Go","model":"deepseek-v4.1-flash"},"critique_2":{"provider":"OpenCode-Go","model":"deepseek-v4.1-flash"},"critique_3":{"provider":"OpenCode-Go","model":"deepseek-v4.1-flash"},"rebuttal":{"provider":"OpenCode-Go","model":"deepseek-v4.1-flash"},"brief":{"provider":"OpenCode-Go","model":"deepseek-v4.1-flash"}}' "Should we launch on web first?"
```

The shell quoting above is for Bash/Zsh. Use canonical provider names in the map: `Mistral`, `OpenRouter`, `Ollama-Cloud`, or `OpenCode-Go`. All six keys are required: `opening`, `critique_1`, `critique_2`, `critique_3`, `rebuttal`, and `brief`. Change each assignment independently. The chosen model must be available to your account and compatible with Council's policy.

## Rounds and reports

```bash
uv run council.py --rounds 2 --out reports "Should we expand the team?"
```

With six assigned roles, each extra round adds three critiques and one rebuttal. More rounds increase calls, cost, and waiting time. The report contains the brief first, then the full debate. `--open` opens the finished brief in a browser.

`--out` takes precedence over the `COUNCIL_REPORTS_DIR` default. Session checkpoints and event files are saved alongside the report; see [saved files](configuration.md#documents-and-output).

## Integration and troubleshooting

```bash
uv run council.py --help
uv run council.py --list-models
uv run council.py --models-json
```

`--json-stream` emits NDJSON events for the UI. `--request-id` links transport activity to a request, and `--resume-json` accepts completed turns for recovery. These are integration options; the desktop app and bridge assemble them automatically.

`--max-attempts` accepts only `1`. SDK retries are also disabled. There are no automatic continuation or summarizer calls. See [diagnostics](agents/diagnostics.md) for request evidence, token policy, and failure investigation.

The prompts and legacy provider order are defined in `council.py`. Prefer model/role configuration over changing the code for an ordinary run.
