#!/usr/bin/env python3
"""Strict CodeLimit gate: fail if any function exceeds MAX_LINES lines of code.

CodeLimit's own `check` only warns above 30 and fails above 60. This applies the
strict band (default 15 = "easy") using CodeLimit's own measurement algorithm.

Usage: check_function_length.py [max_lines] [paths...]
"""

from __future__ import annotations

import sys
from pathlib import Path

from codelimit.commands.check import Languages, get_lexer_for_filename, lex, scan_file

DEFAULT_MAX_LINES = 15
SKIP_DIRS = {".venv", ".git", "build", "dist", "__pycache__", ".ruff_cache", "node_modules"}


def measurements(path: Path):
    """Measure every function in one file, or return nothing for non-source files."""
    try:
        lexer = get_lexer_for_filename(path)
    except Exception:
        return []
    language = Languages.by_name.get(lexer.__class__.name)
    if language is None:
        return []
    return scan_file(lex(lexer, path.read_text(encoding="utf-8"), False), language)


def python_files(roots: list[Path]):
    files = [root for root in roots if root.is_file() and root.suffix == ".py"]
    for root in roots:
        if root.is_dir():
            files.extend(p for p in root.rglob("*.py") if not any(part in SKIP_DIRS for part in p.parts))
    return files


def threshold(argv: list[str]) -> int:
    return int(argv[1]) if len(argv) > 1 and argv[1].isdigit() else DEFAULT_MAX_LINES


def roots(argv: list[str]) -> list[Path]:
    return [Path(p) for p in argv[1:] if not p.isdigit()] or [Path(".")]


def violations(files: list[Path], max_lines: int):
    return [(path, m) for path in files for m in measurements(path) if m.value > max_lines]


def report(over, file_count: int, max_lines: int) -> None:
    for path, m in sorted(over, key=lambda item: -item[1].value):
        print(f"{path}:{m.start.line}: {m.value} lines — {m.unit_name} (max {max_lines})")
    print(f"{file_count} files checked, {len(over)} function(s) over {max_lines} lines.")


def main(argv: list[str]) -> int:
    max_lines = threshold(argv)
    files = python_files(roots(argv))
    over = violations(files, max_lines)
    report(over, len(files), max_lines)
    return 1 if over else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
