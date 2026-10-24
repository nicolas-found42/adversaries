"""Role routing, recovery, and catalog behavior without paid model calls."""

import json

import pytest

import council


def assignments():
    return {role: {"provider": "Mistral", "model": f"custom-{role}"} for role in council.ROLE_NAMES}


def routed_session(rounds=1):
    run = council.Session("Choose A or B?", "", {"Mistral": council.call_fake("Mistral")}, rounds)
    run.assignments = assignments()
    run.dry_run = True
    return run


def test_every_role_can_use_same_provider_with_distinct_models():
    turns = council.run_council(routed_session())
    assert len(turns) == 6
    assert [t.model for t in turns] == [f"Mistral / custom-{r}" for r in council.ROLE_NAMES]
    assert turns[-1].role == "Brief"


def test_resume_does_not_repeat_completed_calls(monkeypatch):
    run = routed_session()
    completed = council.run_council(routed_session())[:3]
    run.turns = completed.copy()
    called = []
    monkeypatch.setattr(council, "_assigned_caller", lambda s, _: lambda *_: called.append(s["model"]) or "C1: x")
    council.run_council(run)
    assert called == ["custom-critique_3", "custom-rebuttal", "custom-brief"]
    assert run.turns[:3] == completed


def test_multiple_rounds_keep_role_assignments():
    turns = council.run_council(routed_session(2))
    assert len(turns) == 10
    assert [t.model for t in turns[1:5]] == [t.model for t in turns[5:9]]


def test_assignment_validation_rejects_missing_roles():
    with pytest.raises(ValueError, match="Assignments must cover"):
        council._parse_assignments(json.dumps({"opening": assignments()["opening"]}))


def test_assignment_validation_accepts_arbitrary_model_ids():
    assert council._parse_assignments(json.dumps(assignments())) == assignments()


def test_model_catalog_failure_keeps_configured_default(monkeypatch):
    monkeypatch.setattr(council, "_provider_key", lambda _: "secret-test-value")
    monkeypatch.setattr(council, "_fetch_models", lambda *_: (_ for _ in ()).throw(ValueError("unavailable")))
    result = council._catalog_entry("Mistral")
    assert result["ready"] is True
    assert result["models"] == [council.MISTRAL_MODEL]
    assert "unavailable" in result["error"]


def test_error_messages_redact_keys(monkeypatch):
    monkeypatch.setattr(council, "_provider_key", lambda _: "secret-test-value")
    assert "secret-test-value" not in council._safe_error(ValueError("bad secret-test-value"))


def test_live_call_limit_has_no_sdk_retries(monkeypatch):
    captured = {}
    monkeypatch.setattr("openai.OpenAI", lambda **kw: captured.update(kw))
    council._assigned_caller(assignments()["opening"], False)
    assert captured["max_retries"] == 0
    assert captured["timeout"] == 90


def test_claims_cannot_self_declare_supplied_without_matching_excerpt():
    assert "SOURCE UNCONFIRMED" in council._ground_claim_line("C1: [SUPPLIED] 42 points.", "five people")
    matched = council._ground_claim_line('C1: [SUPPLIED] Five people | source: "five people"', "five people")
    assert "[SOURCE EXCERPT MATCHED]" in matched


def test_resume_rejects_changed_model_selection():
    run = routed_session()
    run.turns = council.run_council(routed_session())[:1]
    run.assignments["opening"]["model"] = "different"
    with pytest.raises(ValueError, match="do not match"):
        council.run_council(run)


def test_failure_checkpoints_completed_work_and_role_config(tmp_path, monkeypatch):
    args = council.build_parser().parse_args(["--dry-run", "--out", str(tmp_path), "test"])
    args.assignments = assignments()
    monkeypatch.setattr(council, "_assigned_caller", failing_caller)
    with pytest.raises(ValueError, match="provider down"):
        council._run_and_report(args, "test", "", {"Mistral": council.call_fake("Mistral")})
    data = json.loads(next(tmp_path.glob("*.json")).read_text())
    assert len(data["turns"]) == 2
    assert data["brief"] == ""
    assert data["run_config"]["roles"] == assignments()


def failing_caller(selection, _dry_run):
    def call(*_):
        if selection["model"] == "custom-critique_2":
            raise ValueError("provider down")
        return "C1: [ASSUMPTION] Placeholder"

    return call


def test_model_cannot_forge_a_verified_source_label():
    text = "C1: [SOURCE EXCERPT MATCHED] Made-up team metrics"
    assert "SOURCE UNCONFIRMED" in council._ground_claim_line(text, "five people")
