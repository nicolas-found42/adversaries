# Adversarial Council

Ask a decision question once. It runs a red-team chain across three AI vendors and gives you a one-page executive decision brief.

```
Claude (opening position)
   → Gemini   attacks Claude's arguments
   → OpenAI   attacks Gemini's critique
   → Claude   concedes / rebuts, revises its recommendation
   → Claude   writes the Decision Brief (with the full debate attached as an appendix)
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
3. Copy `.env.example` to `.env` and paste your three API keys:
   - Anthropic: https://console.anthropic.com
   - OpenAI: https://platform.openai.com/api-keys
   - Gemini: https://aistudio.google.com/apikey
4. Check which models your keys can use, and set the ones you want in `.env`:
   ```
   uv run council.py --list-models
   ```

## Development

The project uses [uv](https://docs.astral.sh/uv/), [Ruff](https://docs.astral.sh/ruff/) (lint + format), and [ty](https://docs.astral.sh/ty/) (type check), configured in `pyproject.toml`. [CodeLimit](https://getcodelimit.github.io/) guards function length.

```
uv sync --all-extras        # install dependencies
uv run ruff check --fix     # lint (auto-fix)
uv run ruff format          # format
uv run ty check             # type check
uv run codelimit check .    # function-length guard
uv run council.py --dry-run "test"   # smoke test, no API calls
```

CI runs the same checks on every push/PR. Pre-commit hooks (`uv run pre-commit install`) run Ruff and CodeLimit locally before each commit.

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
- **Pay most attention to "Unresolved disagreements".** When three different models still disagree after the debate, that is usually where your own judgement or more research is needed.
- **Cost:** one round is 5 model calls, usually a few cents to about $1 depending on the models and how much context you include.
- **Change the order or roles:** edit `run_council()` in `council.py`. The prompts are plain text at the top of the file.

*This is decision support, not a decision. Check facts and figures before acting.*
