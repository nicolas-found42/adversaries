"""Automatic policy and diagnostic recovery, using SDK HTTP mocks, never paid calls."""

import json
from types import SimpleNamespace

import httpx2 as httpx
import pytest
from openai import OpenAI

import council
import council_inference as inference
import council_models as models
from council_diagnostics import Journal, read_events


@pytest.fixture
def isolated_cache(tmp_path, monkeypatch):
    monkeypatch.setattr(models, "CACHE_DIR", tmp_path / "cache")
    return tmp_path


def router_model():
    return {
        "id": "model",
        "context_length": 131072,
        "top_provider": {"max_completion_tokens": 32768},
        "supported_parameters": ["reasoning"],
        "reasoning": {"supported_efforts": ["high", "low", "none"]},
    }


def response(content="Final answer", finish="stop"):
    return {
        "id": "response-1",
        "model": "actual-model",
        "choices": [
            {
                "index": 0,
                "finish_reason": finish,
                "message": {"role": "assistant", "content": content, "reasoning_content": "PRIVATE THINKING"},
            }
        ],
        "usage": response_usage(),
    }


def response_usage():
    return {
        "prompt_tokens": 100,
        "completion_tokens": 200,
        "completion_tokens_details": {"reasoning_tokens": 150},
        "cost": 0.001,
    }


def mock_client(payload, requests):
    def handle(request):
        requests.append(request)
        if request.url.path.endswith("/models"):
            return httpx.Response(200, json={"object": "list", "data": [router_model()]})
        return httpx.Response(200, json=payload, headers={"x-request-id": "request-1"})

    return OpenAI(
        base_url="https://example.test/v1",
        api_key="fixture",
        max_retries=0,
        http_client=httpx.Client(transport=httpx.MockTransport(handle)),
    )


def run_mock(payload):
    requests = []
    client = mock_client(payload, requests)
    result = inference.complete(client, "model", [{"role": "user", "content": "Decision?"}], "OpenRouter")
    return result, requests


def test_model_limit_and_minimum_positive_effort_sent(isolated_cache, monkeypatch):
    monkeypatch.setenv("COUNCIL_MAX_TOKENS", "4000")
    monkeypatch.setenv("COUNCIL_MAX_ATTEMPTS", "4")
    result, requests = run_mock(response())
    body = json.loads(requests[-1].content)
    assert body["max_tokens"] == 32768
    assert body["reasoning"] == {"enabled": True, "effort": "low"}
    assert body["provider"]["require_parameters"] is True
    assert len(requests) == 2  # One metadata read, one inference.
    assert result.text == "Final answer"
    assert result.diagnostics["usage"]["completion_tokens_details"]["reasoning_tokens"] == 150
    assert result.diagnostics["request_id"] == "request-1"
    assert result.diagnostics["cost"]["source"] == "provider-reported"
    assert "PRIVATE THINKING" not in json.dumps(result.diagnostics)


@pytest.mark.parametrize(("content", "finish"), [("Partial", "length"), (None, "stop"), ("", "length")])
def test_incomplete_response_is_not_retried(isolated_cache, content, finish):
    requests = []
    client = mock_client(response(content, finish), requests)
    with pytest.raises(inference.IncompleteCompletionError) as caught:
        inference.complete(client, "model", [{"role": "user", "content": "x"}], "OpenRouter")
    assert len([r for r in requests if r.url.path.endswith("/chat/completions")]) == 1
    assert caught.value.result.text == (content or "")
    assert "PRIVATE THINKING" not in str(caught.value)


def test_context_overflow_prevents_paid_request(isolated_cache):
    cap = models.normalize("OpenRouter", "model", router_model())
    with pytest.raises(models.ContextOverflowError, match="no request sent"):
        models.request_policy(cap, [{"content": "x" * 131072}])
    parameters, details = models.request_policy(cap, [{"content": "x" * 110000}])
    assert parameters["max_tokens"] == 131072 - 110000 - 320
    assert details["estimate_method"].startswith("conservative")


def test_unknown_limits_use_provider_defaults():
    parameters, details = models.request_policy(
        models.Capabilities("Other", "custom", reasoning=False), [{"content": "x"}]
    )
    assert "max_tokens" not in parameters
    assert details["output_allowance"] is None
    assert details["reasoning_mode"] == "not-supported"


@pytest.mark.parametrize(("model", "expected"), [("mistral-small-latest", "high"), ("magistral-small", None)])
def test_mistral_positive_or_native_reasoning(model, expected):
    cap = models.normalize("Mistral", model, {"capabilities": {"reasoning": True}})
    parameters, details = models.request_policy(cap, [{"content": "x"}])
    assert parameters.get("extra_body", {}).get("reasoning_effort") == expected
    assert details["reasoning_mode"] == (expected or "fixed-native-reasoning")


def test_ollama_show_selects_actual_advertised_minimum():
    raw = {
        "thinking": {"values": [False, "high", "low", "max"]},
        "capabilities": ["completion", "thinking", "tools", "vision"],
        "model_info": {"general.architecture": "deepseek", "deepseek.context_length": 1048576},
    }
    cap = models.normalize("Ollama-Cloud", "deepseek", raw)
    parameters, details = models.request_policy(cap, [{"content": "x"}])
    assert parameters["extra_body"]["reasoning_effort"] == "low"
    assert details["capabilities"]["context_window"] == 1048576


def test_reasoning_budget_minimum_and_nonreasoning():
    cap = models.normalize("OpenRouter", "budget-model", {"reasoning": {"budget": {"min": 1024}}})
    parameters, _ = models.request_policy(cap, [{"content": "x"}])
    assert parameters["extra_body"]["reasoning"]["max_tokens"] == 1024
    plain = models.normalize("OpenRouter", "plain", {"supported_parameters": []})
    assert models.request_policy(plain, [{"content": "x"}])[0] == {}


def test_request_journal_survives_with_no_keys_or_reasoning(isolated_cache, monkeypatch):
    monkeypatch.setenv("TEST_API_KEY", "secret-fixture-value")
    journal = Journal(isolated_cache / "run.events.jsonl")
    with inference.recording(journal):
        run_mock(response("secret-fixture-value"))
    events = read_events(journal.path)
    assert [e["event"] for e in events] == ["request_started", "request_finished"]
    assert events[-1]["actual_model"] == "actual-model"
    assert "PRIVATE THINKING" not in journal.path.read_text()
    assert "secret-fixture-value" not in journal.path.read_text()
    assert journal.path.stat().st_mode & 0o777 == 0o600
    snapshot = events[0]["request_snapshot"]
    assert json.loads(__import__("pathlib").Path(snapshot).read_text())["messages"][0]["content"] == "Decision?"


def partial_caller(*_):
    raise inference.IncompleteCompletionError(
        "Output allowance exhausted", inference.Completion("Partial", {"finish_reason": "length"})
    )


def test_partial_is_saved_but_never_passed_to_next_stage(tmp_path):
    session = council.Session("q", "", {"model": partial_caller}, checkpoint=tmp_path / "run.md")
    session.journal = Journal(tmp_path / "run.events.jsonl")
    with pytest.raises(inference.IncompleteCompletionError):
        council.run_turn(session, "model", "Opening", "system", "user")
    saved = json.loads((tmp_path / "run.json").read_text())
    assert session.turns == []
    assert saved["failed_turn"]["text"] == "Partial"
    assert saved["failed_turn"]["status"] == "incomplete"
    assert "Partial" not in council.transcript_text(session)
    assert saved["diagnostics"]["events"][-1]["event"] == "stage_failed"


def test_error_status_has_one_request_and_persistent_failure(isolated_cache, monkeypatch):
    monkeypatch.setattr(inference, "discover", lambda *_: models.Capabilities("OpenRouter", "custom", reasoning=False))
    requests = []

    def handle(request):
        requests.append(request)
        return httpx.Response(429, json={"error": {"message": "Rate limit", "type": "rate_limit"}})

    client = OpenAI(api_key="fixture", max_retries=0, http_client=httpx.Client(transport=httpx.MockTransport(handle)))
    with (
        inference.recording(Journal(isolated_cache / "error.events.jsonl")),
        pytest.raises(inference.ProviderRequestError),
    ):
        inference.complete(client, "custom", [{"content": "x", "role": "user"}], "OpenRouter")
    assert sum(r.url.path.endswith("/chat/completions") for r in requests) == 1
    assert read_events(isolated_cache / "error.events.jsonl")[-1]["status_code"] == 429


def test_resume_keeps_diagnostics_and_rejects_partial():
    record = {"model": "m", "role": "r", "text": "t", "seconds": 1, "diagnostics": {"request_id": "id"}}
    assert council._resume_turns(json.dumps([record]))[0].diagnostics["request_id"] == "id"
    with pytest.raises(ValueError, match="Only completed"):
        council._resume_turns(json.dumps([{**record, "status": "incomplete"}]))


def test_go_flash_uses_low_and_does_not_invent_output_limit():
    cap = models.normalize("OpenCode-Go", "deepseek-v4.1-flash", {"id": "deepseek-v4.1-flash"})
    params, details = models.request_policy(cap, [{"content": "x"}])
    assert params["extra_body"]["reasoning_effort"] == "low"
    assert "max_tokens" not in params
    assert "Go lists IDs only" in details["capabilities"]["warnings"][0]


def test_ollama_metadata_reads_show(monkeypatch, isolated_cache):
    seen = []
    client = SimpleNamespace(post=lambda url, **kw: seen.append((url, kw)) or {"thinking": {"values": [False, True]}})
    cap = models.discover("Ollama-Cloud", "boolean-thinking", client)
    assert seen[0][1]["body"] == {"model": "boolean-thinking"}
    assert models.request_policy(cap, [{"content": "x"}])[0]["extra_body"]["reasoning_effort"] == "low"


def test_unknown_reasoning_never_sends_uncontrolled_request():
    with pytest.raises(models.PolicyUnavailableError, match="no inference sent"):
        models.request_policy(models.Capabilities("Other", "custom"), [{"content": "x"}])


def test_inline_thinking_is_not_a_final_answer():
    assert inference._final_text("<think>private</think>Answer") == "Answer"
    assert inference._final_text("<think>private only") == ""


def test_request_id_survives_incomplete_checkpoint(isolated_cache):
    requests = []
    client = mock_client(response("Partial", "length"), requests)
    session = council.Session(
        "q", "", {"m": lambda *_: inference.complete(client, "model", [{"content": "x", "role": "user"}], "OpenRouter")}
    )
    with pytest.raises(inference.IncompleteCompletionError):
        council.run_turn(session, "m", "Opening", "system", "user")
    assert session.failed_turn is not None
    assert session.failed_turn.diagnostics["request_id"] == "request-1"


@pytest.mark.parametrize(
    "caller", [council.call_mistral, council.call_openrouter, council.call_ollama_cloud, council.call_opencode_go]
)
def test_legacy_clients_also_have_no_sdk_retries(caller, monkeypatch):
    captured = {}
    monkeypatch.setattr("openai.OpenAI", lambda **kw: captured.update(kw) or object())
    monkeypatch.setattr(council, "_chat_completion", lambda *_: inference.Completion("fixture"))
    caller("system", "user")
    assert captured["max_retries"] == 0
    assert captured["timeout"] == 90


def test_minimum_reasoning_budget_must_leave_answer_room():
    cap = models.Capabilities("OpenRouter", "budget", output_limit=1024, reasoning=True, minimum_budget=1024)
    with pytest.raises(models.ContextOverflowError, match="minimum reasoning budget"):
        models.request_policy(cap, [{"content": "x"}])


def test_mistral_thinking_chunks_are_not_final_content():
    content = [{"type": "thinking", "thinking": "private"}, {"type": "text", "text": "Answer"}]
    assert inference._final_text(content) == "Answer"
    assert inference._has_reasoning({"content": content}) is True


def test_ollama_nonreasoning_capability_list():
    cap = models.normalize("Ollama-Cloud", "plain", {"capabilities": ["completion", "tools"]})
    assert cap.reasoning is False
    assert models.request_policy(cap, [{"content": "x"}])[0] == {}
