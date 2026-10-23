# Quality gates

Every hook in `.pre-commit-config.yaml` is a **wall**: it fails the run. CI invokes the same hooks and the hook entries own the flags, so local green is the only result worth reporting.

The bar: `uv run pre-commit run --all-files` exits 0.

## While iterating

Run the one wall you are fixing:

```
uv run pre-commit run <hook-id> --all-files
```

`ruff-check` and `ruff-format` rewrite the file and still fail the run; stage the rewrite and run again to see green.

## Which wall owns the failure

| Symptom | Wall | Move |
|---|---|---|
| function over 15 code lines | `codelimit-strict` | extract a helper named for the steps it performs |
| CCN over 10 | `lizard` | one predicate per branch; early returns flatten a nest |
| more than 5 parameters | `lizard` | bundle co-travelling arguments in a dataclass — `Session` is the precedent |
| type error | `ty` | widen the annotation or narrow the value; a scoped `# type: ignore[code]` marks a third-party stub gap |
| lint finding | `ruff-check` | `--fix` first, then read what remains; a scoped `# noqa: CODE` carries a reason |
| forbidden import | `import-linter` | repository tooling imports nothing from `council` |
| failing test | `pytest` | see below |

## Policy

A wall marks a design smell, so the fix is in the code. Change a limit only when the gate names a function that cannot be split: say which and why in the commit message, then edit the hook entry — the single place local runs and CI read.

## pytest

Tests drive `run_council` through `council.call_fake`, with no API keys and no network. Extend `tests/test_council.py`.

Assert what a consumer sees — turn order, the target of `attack_prompt`, what the report holds — and carry the same walls as the app: ≤15 lines, ≤5 parameters.

## Advisory

`radon` and `xenon` run in CI with `continue-on-error`. A rank of C is a prompt to read that function; no gate fails on it.
