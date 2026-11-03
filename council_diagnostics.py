"""Local, content-minimal event history plus private request snapshots."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import platform
import re
import sys
import uuid
from dataclasses import dataclass, field
from pathlib import Path

SCHEMA_VERSION = 1


def fingerprint(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def scrub(value):
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items() if not _secret_field(k)}
    if isinstance(value, list):
        return [scrub(item) for item in value]
    return _scrub_text(value) if isinstance(value, str) else value


# Matched against whole words only, so `completion_tokens` stays a token count
# while `OPENROUTER_API_KEY`, `GITHUB_TOKEN`, and `DB_PASSWORD` are secrets.
_SECRET_WORDS = frozenset(
    {"key", "apikey", "authorization", "cookie", "credential", "credentials", "password", "passwd", "secret", "token"}
)
_WORDS = re.compile(r"[^a-z0-9]+")
_PRIVATE_FLAGS = os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, "O_NOFOLLOW", 0)


def _secret_field(key: str) -> bool:
    return any(word in _SECRET_WORDS for word in _WORDS.split(key.lower()))


def _scrub_text(text: str) -> str:
    for name, value in os.environ.items():
        if len(value) >= 8 and _secret_field(name):
            text = text.replace(value, "<redacted>")
    return re.sub(r"(?i)bearer\s+[^\s\"']+", "Bearer <redacted>", text)


def private_directory(path: Path) -> None:
    """Create an owner-only directory; a directory that already exists is left untouched."""
    try:
        path.mkdir(parents=True, mode=0o700)
    except FileExistsError:
        return
    os.chmod(path, 0o700)


def private_write(path: Path, data: dict) -> None:
    private_text(path, json.dumps(scrub(data), ensure_ascii=False, indent=2))


def private_text(path: Path, text: str, *, exclusive: bool = False) -> None:
    """Write scrubbed text at mode 0600. A logical write is replaced atomically unless exclusive."""
    private_directory(path.parent)
    cleaned = _scrub_text(text)
    if exclusive:
        _write_exclusive(path, cleaned)
        return
    temporary = path.with_suffix(path.suffix + ".tmp")
    try:
        _write_private_file(temporary, cleaned)
        temporary.replace(path)
    except OSError:
        temporary.unlink(missing_ok=True)
        raise


def _write_private_file(path: Path, text: str) -> None:
    descriptor = os.open(path, _PRIVATE_FLAGS, 0o600)
    _write_descriptor(descriptor, text)


def _write_exclusive(path: Path, text: str) -> None:
    descriptor = os.open(path, _PRIVATE_FLAGS | os.O_EXCL, 0o600)
    try:
        _write_descriptor(descriptor, text)
    except OSError:
        path.unlink(missing_ok=True)
        raise


def _write_descriptor(descriptor: int, text: str) -> None:
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        tighten = getattr(os, "fchmod", None)  # Absent on Windows; the mode flag already covers POSIX creation.
        if tighten:
            tighten(descriptor, 0o600)
        handle.write(text)
        handle.flush()
        os.fsync(handle.fileno())


def read_events(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    records = []
    for line in path.read_text(encoding="utf-8").splitlines():
        try:
            records.append(json.loads(line))
        except ValueError:
            continue  # A process can be killed while writing its final record.
    return records


@dataclass
class Journal:
    path: Path
    run_id: str = field(default_factory=lambda: uuid.uuid4().hex)
    sequence: int = 0

    def record(self, event: str, **fields) -> dict:
        self.sequence += 1
        data = {
            "schema_version": SCHEMA_VERSION,
            "run_id": self.run_id,
            "seq": self.sequence,
            "time": dt.datetime.now(dt.UTC).isoformat(),
            "event": event,
            **fields,
        }
        _append_event(self.path, scrub(data))
        return scrub(data)

    def snapshot(self, call_id: str, messages: list[dict]) -> str:
        path = self.path.parent / self.run_id / f"{call_id}.request.json"
        private_write(path, {"run_id": self.run_id, "call_id": call_id, "messages": messages})
        return str(path)


def environment_facts() -> dict:
    roots = [Path(__file__).with_name(name) for name in ("council.py", "council_inference.py", "council_models.py")]
    versions = {path.name: fingerprint(path.read_text()) for path in roots if path.is_file()}
    return {"python": sys.version.split()[0], "platform": platform.system(), "source_sha256": versions}


def session_diagnostics(journal: Journal | None) -> dict:
    if journal is None:
        return {}
    return {
        "schema_version": SCHEMA_VERSION,
        "run_id": journal.run_id,
        "events": read_events(journal.path),
    }


def _append_event(path: Path, data: dict) -> None:
    private_directory(path.parent)
    flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0)
    _write_descriptor(os.open(path, flags, 0o600), json.dumps(scrub(data), ensure_ascii=False) + "\n")
