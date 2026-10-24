# Documentation

Start with the [project README](../README.md) to try the desktop app and run your first decision.

## Using Council

| Guide | What you will find |
| --- | --- |
| [Using Council](usage.md) | Debate flow, rounds, history, recovery, claim labels, and keyboard shortcuts |
| [Provider setup and configuration](configuration.md) | API keys, models, attachments, output locations, and privacy |
| [Command-line guide](cli.md) | Questions from files, context, provider selection, and per-role assignments |
| [Diagnostics](agents/diagnostics.md) | Output and reasoning policy, token usage, audit files, retention, and troubleshooting |

## Developing Council

- [Development guide](development.md): desktop and browser builds, engine integration, and quality checks.
- [Quality gate conventions](agents/quality-gates.md): limits, hook ownership, and how to fix failures.
- [Issue tracker](agents/issue-tracker.md) and [triage labels](agents/triage-labels.md).
- [Domain conventions](agents/domain.md) and the [glossary](../GLOSSARY.md).

## Research

These notes record findings at the date of research; provider capabilities and available models can change.

- [UI landscape](research/2026-10-23-ui-landscape.md).
- [Rust UI and browser live reload](research/2026-10-23-rust-ui-live-reload.md).
- [Output limits, context overflow, and reasoning](research/2026-10-23-token-budgets-and-reasoning.md).
- [Customer demo production and open-source tools](research/2026-10-23-customer-demo-production.md).

README visuals come from the real six-call demo run using fictional launch context. See [image provenance](images/README.md).
