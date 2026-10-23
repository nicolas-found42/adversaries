"""Behavioral tests for the council pipeline. No API calls: callers are fakes."""

from pathlib import Path

import council

NAMES = ("Mistral", "OpenRouter", "Ollama-Cloud", "OpenCode-Go")


def session(rounds: int = 1, context: str = "") -> council.Session:
    return council.Session("Hire or outsource?", context, {name: council.call_fake(name) for name in NAMES}, rounds)


def test_one_round_runs_opening_attacks_rebuttal_and_brief():
    turns = council.run_council(session())
    assert [(t.model, t.role) for t in turns] == [
        ("Mistral", "Opening"),
        ("OpenRouter", "Red team"),
        ("Ollama-Cloud", "Red team"),
        ("OpenCode-Go", "Red team"),
        ("Mistral", "Rebuttal"),
        ("Mistral", "Brief"),
    ]


def test_each_extra_round_adds_four_turns():
    assert len(council.run_council(session(rounds=2))) == 10


def test_attack_prompt_targets_only_the_latest_output():
    run = session()
    council.run_turn(run, "Mistral", "Opening", council.PROPOSER, "q")
    council.run_turn(run, "OpenRouter", "Red team", council.ATTACKER, "q")
    target = run.turns[-1]
    prompt = council.attack_prompt(run)
    assert prompt.count(target.text) == 1
    assert "## TURN 2" not in prompt
    assert f"OUTPUT TO ATTACK — {target.model} ({target.role})" in prompt


def test_attack_prompt_keeps_every_earlier_turn():
    run = session()
    council.run_turn(run, "Mistral", "Opening", council.PROPOSER, "q")
    council.run_turn(run, "OpenRouter", "Red team", council.ATTACKER, "q")
    council.run_turn(run, "Ollama-Cloud", "Red team", council.ATTACKER, "q")
    prompt = council.attack_prompt(run)
    assert "## TURN 1" in prompt
    assert "## TURN 2" in prompt
    assert "## TURN 3" not in prompt


def test_transcript_defaults_to_session_turns():
    run = session()
    council.run_turn(run, "Mistral", "Opening", council.PROPOSER, "q")
    assert "## TURN 1" in council.transcript_text(run)
    assert "## TURN 1" not in council.transcript_text(run, [])


def test_context_only_appears_when_given():
    assert "BACKGROUND" not in council.transcript_text(session(), [])
    assert "BACKGROUND" in council.transcript_text(session(context="budget 5m"), [])


def test_report_has_brief_header_and_one_entry_per_debate_turn(tmp_path: Path):
    turns = council.run_council(session())
    path = council.write_report("Hire or outsource?", turns, tmp_path, rounds=1)
    body = path.with_suffix(".md").read_text(encoding="utf-8")
    assert body.startswith(turns[-1].text)
    assert "Hire or outsource?" in body
    assert body.count("\n## Turn ") == len(turns) - 1


def test_dry_run_writes_a_brief(tmp_path: Path):
    args = council.build_parser().parse_args(["--dry-run", "--out", str(tmp_path), "Hire or outsource?"])
    council._run_and_report(args, "Hire or outsource?", "", council.resolve_callers(args.dry_run))
    assert list(tmp_path.glob("*.md"))


def test_resolve_callers_dry_run_covers_all_four_providers():
    callers = council.resolve_callers(dry_run=True)
    assert set(callers.keys()) == {"Mistral", "OpenRouter", "Ollama-Cloud", "OpenCode-Go"}


def test_model_sources_lists_all_four_providers():
    sources = [name for name, _ in council._model_sources()]
    assert sources == ["Mistral", "OpenRouter", "Ollama-Cloud", "OpenCode-Go"]


def test_generation_note_includes_all_providers():
    note = council._generation_note(1)
    for name in ("Mistral=", "OpenRouter=", "Ollama-Cloud=", "OpenCode-Go="):
        assert name in note


def test_skip_provider_runs_remaining_council_turns():
    providers = ("Mistral", "OpenRouter", "OpenCode-Go")
    callers = {name: council.call_fake(name) for name in providers}
    run = council.Session("q", "", callers, rounds=1, providers=providers)
    turns = council.run_council(run)
    assert [(t.model, t.role) for t in turns] == [
        ("Mistral", "Opening"),
        ("OpenRouter", "Red team"),
        ("OpenCode-Go", "Red team"),
        ("Mistral", "Rebuttal"),
        ("Mistral", "Brief"),
    ]


def test_resolve_providers_filters_skipped():
    args = council.build_parser().parse_args(["--skip", "ollama-cloud", "test"])
    assert council._resolve_providers(args) == ("Mistral", "OpenRouter", "OpenCode-Go")
