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


def _secret_field(key: str) -> bool:
    return any(part in key.lower() for part in ("api_key", "authorization", "password", "access_token"))


def _scrub_text(text: str) -> str:
    for name, value in os.environ.items():
        if _secret_field(name) and len(value) >= 8:
            text = text.replace(value, "<redacted>")
    return re.sub(r"(?i)bearer\s+[^\s\"']+", "Bearer <redacted>", text)


def private_write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temp = path.with_suffix(path.suffix + ".tmp")
    with temp.open("w", encoding="utf-8") as handle:
        os.chmod(temp, 0o600)
        json.dump(scrub(data), handle, ensure_ascii=False, indent=2)
        handle.flush()
        os.fsync(handle.fileno())
    temp.replace(path)


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
        "journal_path": str(journal.path),
        "events": read_events(journal.path),
    }


def _append_event(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with path.open("a", encoding="utf-8") as handle:
        os.chmod(path, 0o600)
        handle.write(json.dumps(scrub(data), ensure_ascii=False) + "\n")
        handle.flush()
        os.fsync(handle.fileno())
