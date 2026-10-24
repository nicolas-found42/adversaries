"""Provider metadata and automatic, capability-driven request policy."""

from __future__ import annotations

import json
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

from council_diagnostics import fingerprint, private_write, scrub

CACHE_DIR = Path(__file__).parent / ".council-cache"
CACHE_SECONDS = 3600


@dataclass
class Capabilities:
    provider: str
    model: str
    context_window: int | None = None
    output_limit: int | None = None
    reasoning: bool | None = None
    efforts: list[str] = field(default_factory=list)
    reasoning_toggle: bool = False
    minimum_budget: int | None = None
    fixed_reasoning: bool = False
    source: str = "provider-default"
    warnings: list[str] = field(default_factory=list)
    output_parameter: str = "max_tokens"
    pricing: dict = field(default_factory=dict)
    metadata_error: dict = field(default_factory=dict)
    retrieved_at: float | None = None


def positive_minimum(values: list) -> str | None:
    positive = [
        v for v in values if isinstance(v, str) and v.lower() not in {"none", "off", "disabled", "default", "auto"}
    ]
    order = {v: i for i, v in enumerate(("minimal", "low", "medium", "high", "xhigh", "max", "ultra"))}
    return min(positive, key=lambda v: order.get(v, 100)) if positive else None


def normalize(provider: str, model: str, raw: dict) -> Capabilities:
    cap = Capabilities(provider, model, source="provider-model-metadata")
    cap.context_window = _positive(
        raw.get("context_window") or raw.get("max_context_length") or raw.get("context_length")
    )
    cap.output_limit = _positive(
        raw.get("max_output_tokens") or (raw.get("top_provider") or {}).get("max_completion_tokens")
    )
    cap.pricing = raw.get("pricing") or {}
    _reasoning_metadata(cap, raw)
    if provider == "Ollama-Cloud":
        _ollama_metadata(cap, raw)
    _documented_profiles(cap)
    return cap


def _documented_profiles(cap: Capabilities) -> None:
    if cap.provider == "OpenCode-Go" and cap.model in {
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "deepseek-v4.1-flash",
        "deepseek-v4.1-pro",
    }:
        cap.reasoning, cap.efforts = True, ["low", "high", "max"]
        cap.source += "+deepseek-docs+opencode-catalog"
        cap.warnings.append(
            "Go lists IDs only; reasoning effort requested from documented model profile, not response-verified."
        )
    if cap.context_window is None or cap.output_limit is None:
        cap.warnings.append("Provider does not advertise every limit; unknown limits remain provider defaults.")


def _positive(value) -> int | None:
    return value if isinstance(value, int) and not isinstance(value, bool) and value > 0 else None


def _reasoning_metadata(cap: Capabilities, raw: dict) -> None:
    reasoning = raw.get("reasoning")
    support = _reasoning_support(raw.get("capabilities"))
    if isinstance(support, bool):
        cap.reasoning = support
    if isinstance(reasoning, dict):
        _effort_metadata(cap, reasoning)
    elif cap.provider == "OpenRouter" and "supported_parameters" in raw:
        cap.reasoning = "reasoning" in raw["supported_parameters"] or "reasoning_effort" in raw["supported_parameters"]
    if cap.provider == "Mistral" and cap.reasoning:
        _mistral_efforts(cap)


def _effort_metadata(cap: Capabilities, data: dict) -> None:
    cap.reasoning = data.get("supported", True)
    values = data.get("supported_efforts", data.get("effort", []))
    cap.efforts = ["minimal"] if values is None and cap.provider == "OpenRouter" else (values or [])
    cap.minimum_budget = _positive(data.get("budget", {}).get("min"))
    cap.reasoning_toggle = bool(data.get("toggle"))


def _mistral_efforts(cap: Capabilities) -> None:
    # The capability bit is authoritative; effort mapping is documented per family.
    if cap.model.startswith("magistral-"):
        cap.fixed_reasoning = True
        cap.source += "+mistral-native-reasoning-docs"
    elif cap.model.startswith(("mistral-small", "mistral-medium")):
        cap.efforts = ["high"]
        cap.source += "+mistral-reasoning-docs"
    else:
        cap.warnings.append(
            "Thinking supported, but positive effort controls are not advertised for this hosted model."
        )


def _ollama_metadata(cap: Capabilities, raw: dict) -> None:
    info = raw.get("model_info", {})
    architecture = info.get("general.architecture", "")
    cap.context_window = _positive(info.get(f"{architecture}.context_length")) or cap.context_window
    thinking = raw.get("thinking", {})
    values = thinking.get("values")
    if values is not None:
        cap.reasoning = any(v is True or isinstance(v, str) for v in values)
        cap.efforts = [v for v in values if isinstance(v, str)]
        cap.reasoning_toggle = True in values


def discover(provider: str, model: str, client) -> Capabilities:
    path = CACHE_DIR / (fingerprint(f"{provider}/{model}") + ".json")
    cached = _cached(path)
    try:
        if cached:
            return Capabilities(**cached)
        cap = normalize(provider, model, _fetch_metadata(provider, model, client))
        cap.retrieved_at = time.time()
        private_write(path, {"at": time.time(), "capabilities": asdict(cap)})
        return cap
    except Exception as error:
        return _unknown_capabilities(provider, model, error)


def _cached(path: Path) -> dict | None:
    try:
        data = json.loads(path.read_text())
        return data["capabilities"] if time.time() - data["at"] < CACHE_SECONDS else None
    except OSError, ValueError, KeyError:
        return None


def _fetch_metadata(provider: str, model: str, client) -> dict:
    if provider == "Ollama-Cloud":
        return client.post("https://ollama.com/api/show", body={"model": model}, cast_to=dict)
    records = client.models.list().data
    for record in records:
        data = record.model_dump()
        if model == data["id"] or model in (data.get("aliases") or []):
            return data
    raise ValueError("Selected model is absent from provider metadata")


def request_policy(cap: Capabilities, messages: list[dict]) -> tuple[dict, dict]:
    estimate = sum(len(m["content"].encode()) + 64 for m in messages) + 256
    details = _policy_details(cap, estimate)
    room = _context_room(cap, estimate, details)
    allowance = min(cap.output_limit, room) if cap.output_limit and room else cap.output_limit
    details["output_allowance"] = allowance
    details["output_mode"] = "model-limit" if allowance else "provider-default-unknown"
    _check_reasoning_room(cap, allowance, details)
    parameters = {cap.output_parameter: allowance} if allowance else {}
    parameters.update(reasoning_parameters(cap, details))
    return parameters, details


class ContextOverflowError(ValueError):
    def __init__(self, message: str, diagnostics: dict):
        super().__init__(message)
        self.diagnostics = {"policy": diagnostics, "request_sent": False}


class PolicyUnavailableError(ValueError):
    def __init__(self, message: str, policy: dict):
        super().__init__(message)
        self.diagnostics = {"policy": policy, "request_sent": False}


def reasoning_parameters(cap: Capabilities, details: dict) -> dict:
    effort = positive_minimum(cap.efforts)
    details["reasoning_mode"] = effort or _default_reasoning_mode(cap)
    if cap.fixed_reasoning:
        details["reasoning_mode"] = "fixed-native-reasoning"
        return {}
    if cap.reasoning is None:
        raise PolicyUnavailableError(
            "Reasoning capabilities are unknown; no inference sent. Refresh metadata or choose a documented model.",
            details,
        )
    if not cap.reasoning:
        return {}
    return _positive_reasoning_parameters(cap, details, effort)


def _positive_reasoning_parameters(cap: Capabilities, details: dict, effort: str | None) -> dict:
    if cap.provider == "OpenRouter":
        details["reasoning_budget"] = cap.minimum_budget
        reasoning = _router_reasoning(cap, effort)
        return {"extra_body": {"reasoning": reasoning, "provider": {"require_parameters": True}}}
    if effort:
        return {"extra_body": {"reasoning_effort": effort}}
    if cap.provider == "Ollama-Cloud" and cap.reasoning_toggle:
        return {"extra_body": {"reasoning_effort": "low"}}
    raise PolicyUnavailableError(
        "Model supports thinking but its positive reasoning control is unknown; no inference sent.", details
    )


def _router_reasoning(cap: Capabilities, effort: str | None) -> dict:
    if cap.minimum_budget:
        return {"enabled": True, "max_tokens": cap.minimum_budget}
    if effort:
        return {"enabled": True, "effort": effort}
    return {"enabled": True}


def _policy_details(cap: Capabilities, estimate: int) -> dict:
    return {
        "capabilities": asdict(cap),
        "input_token_estimate": estimate,
        "estimate_method": "conservative-utf8-bytes-plus-framing",
        "output_allowance": None,
    }


def _unknown_capabilities(provider: str, model: str, error: Exception) -> Capabilities:
    cap = Capabilities(provider, model)
    cap.metadata_error = scrub(
        {
            "type": type(error).__name__,
            "message": str(error),
            "status_code": getattr(error, "status_code", None),
            "request_id": getattr(error, "request_id", None),
        }
    )
    _documented_profiles(cap)
    cap.warnings.append(f"Metadata unavailable ({type(error).__name__}); using provider defaults, controls unknown.")
    return cap


def _default_reasoning_mode(cap: Capabilities) -> str:
    if cap.reasoning is None:
        return "unknown-blocked"
    return "enabled" if cap.reasoning else "not-supported"


def clear_capability_cache() -> None:
    for path in CACHE_DIR.glob("*.json"):
        path.unlink(missing_ok=True)


def _check_reasoning_room(cap: Capabilities, allowance: int | None, details: dict) -> None:
    if allowance and cap.minimum_budget and allowance <= cap.minimum_budget:
        raise ContextOverflowError(
            "Output room cannot fit the model's minimum reasoning budget and an answer; no request sent.", details
        )


def _context_room(cap: Capabilities, estimate: int, details: dict) -> int | None:
    room = cap.context_window - estimate if cap.context_window else None
    if room is not None and room <= 0:
        raise ContextOverflowError(
            "Conservative input estimate exceeds advertised context; no request sent. Reduce background or choose a larger-context model.",
            details,
        )
    return room


def _reasoning_support(capabilities) -> bool | None:
    if isinstance(capabilities, dict):
        return capabilities.get("reasoning")
    if isinstance(capabilities, list):
        return "thinking" in capabilities or "reasoning" in capabilities
    return None
