# Adversarial Council

A single-file Python CLI (`council.py`) that runs a red-team debate across Mistral, OpenRouter, Ollama Cloud, and OpenCode Go, then writes an executive decision brief.

## Quality gates

Hooks in `.pre-commit-config.yaml` are walls: `uv run pre-commit run <hook-id> --all-files` while iterating, `uv run pre-commit run --all-files` before calling work done (CI runs the same hooks). Which wall owns which failure: `docs/agents/quality-gates.md`.

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues (`nicolas-found42/adversaries`), managed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles map 1:1 to `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` at the repo root plus `docs/adr/` for decisions. See `docs/agents/domain.md`.
