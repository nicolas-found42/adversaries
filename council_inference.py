"""One inference request, with final-answer validation and durable diagnostics."""

from __future__ import annotations

import time
import traceback
import uuid
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field

from council_diagnostics import Journal, scrub
from council_models import discover, request_policy

ACTIVE: ContextVar[Journal | None] = ContextVar("council_journal", default=None)


@dataclass
class Completion:
    text: str
    diagnostics: dict = field(default_factory=dict)


class IncompleteCompletionError(RuntimeError):
    def __init__(self, message: str, result: Completion):
        super().__init__(message)
        self.result = result
        self.diagnostics = result.diagnostics


class ProviderRequestError(RuntimeError):
    def __init__(self, message: str, diagnostics: dict):
        super().__init__(message)
        self.diagnostics = diagnostics


@contextmanager
def recording(journal: Journal | None):
    token = ACTIVE.set(journal)
    try:
        yield
    finally:
        ACTIVE.reset(token)


def _record(event: str, details: dict) -> None:
    journal = ACTIVE.get()
    if journal:
        journal.record(event, **details)


def complete(client, model: str, messages: list[dict], provider: str) -> Completion:
    cap = discover(provider, model, client)
    parameters, policy = request_policy(cap, messages)
    details = {"call_id": uuid.uuid4().hex, "provider": provider, "selected_model": model, "policy": policy}
    _request_snapshot(details, messages)
    _record("request_started", details)
    started = time.monotonic()
    try:
        response = client.chat.completions.create(model=model, messages=messages, **parameters)
    except Exception as error:
        raise _request_failed(error, {**details, "seconds": time.monotonic() - started}) from error
    result = _response_result(response, {**details, "seconds": time.monotonic() - started})
    _record("request_finished", result.diagnostics)
    return _validate_result(result)


def _request_failed(error: Exception, details: dict) -> ProviderRequestError:
    failure = {**details, **error_facts(error)}
    _record("request_failed", failure)
    return ProviderRequestError(str(error), failure)


def _request_snapshot(details: dict, messages: list[dict]) -> None:
    journal = ACTIVE.get()
    if journal:
        details["request_snapshot"] = journal.snapshot(details["call_id"], messages)


def error_facts(error: BaseException) -> dict:
    return scrub(_error_details(error))


def _error_details(error: BaseException) -> dict:
    return {
        k: v
        for k, v in {
            "error_type": type(error).__name__,
            "message": str(error),
            "status_code": getattr(error, "status_code", None),
            "request_id": getattr(error, "request_id", None),
            "stack": [
                {"file": f.filename, "line": f.lineno, "function": f.name}
                for f in traceback.extract_tb(error.__traceback__)[-10:]
            ],
        }.items()
        if v is not None
    }


def _response_result(response, details: dict) -> Completion:
    raw = response.model_dump()
    choices = raw.get("choices") or []
    choice = choices[0] if choices else {}
    message = choice.get("message") or {}
    details.update(_response_facts(response, raw, choice, message))
    details["cost"] = _cost(details)
    return Completion(_final_text(message.get("content")), scrub(details))


def _final_text(content) -> str:
    if isinstance(content, str):
        return _without_thinking(content.strip())
    if isinstance(content, list):
        return "\n".join(str(part.get("text", "")) for part in content if part.get("type") == "text").strip()
    return ""


def _validate_result(result: Completion) -> Completion:
    reason = result.diagnostics.get("finish_reason")
    native = result.diagnostics.get("native_finish_reason")
    if reason in {"length", "max_tokens"} or native in {"length", "max_tokens", "MAX_TOKENS"}:
        raise IncompleteCompletionError(
            "Output allowance exhausted. Partial answer saved; no automatic continuation.", result
        )
    if not result.text:
        raise IncompleteCompletionError(
            "No final answer returned (possibly reasoning-only). No automatic retry.", result
        )
    if reason not in {"stop", "end_turn"}:
        raise IncompleteCompletionError(f"Response did not finish normally ({reason}); partial answer saved.", result)
    return result


def _cost(details: dict) -> dict:
    usage = details.get("usage") or {}
    if usage.get("cost") is not None:
        return {"amount": usage["cost"], "currency": "USD", "source": "provider-reported"}
    prices = details["policy"]["capabilities"].get("pricing", {})
    try:
        amount = int(usage["prompt_tokens"]) * float(prices["prompt"]) + int(usage["completion_tokens"]) * float(
            prices["completion"]
        )
        return {"amount": amount, "currency": "USD", "source": "catalog-estimate-not-invoice"}
    except KeyError, TypeError, ValueError:
        return {"amount": None, "source": "unavailable"}


def _response_facts(response, raw: dict, choice: dict, message: dict) -> dict:
    return {
        "response_id": raw.get("id"),
        "actual_model": raw.get("model"),
        "request_id": getattr(response, "_request_id", None),
        "finish_reason": choice.get("finish_reason"),
        "native_finish_reason": choice.get("native_finish_reason"),
        "usage": raw.get("usage"),
        "has_reasoning": _has_reasoning(message),
    }


def _without_thinking(content: str) -> str:
    if not content.startswith("<think>"):
        return content
    _, closed, final = content.partition("</think>")
    return final.strip() if closed else ""


def _has_reasoning(message: dict) -> bool:
    if any(message.get(key) for key in ("reasoning_content", "reasoning", "reasoning_details")):
        return True
    content = message.get("content")
    return isinstance(content, list) and any(part.get("type") == "thinking" for part in content)
