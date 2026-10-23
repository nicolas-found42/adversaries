# Adversarial Council

A single-file Python CLI (`council.py`) that runs a red-team debate across Claude, Gemini, and OpenAI, then writes an executive decision brief.

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues (`nicolas-found42/adversaries`), managed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles map 1:1 to `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` at the repo root plus `docs/adr/` for decisions. See `docs/agents/domain.md`.
