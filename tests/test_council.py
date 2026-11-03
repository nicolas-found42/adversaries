"""Behavioral tests for the council pipeline. No API calls: callers are fakes."""

import os
from pathlib import Path

import pytest

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


def test_dry_run_writes_json_sidecar(tmp_path: Path):
    args = council.build_parser().parse_args(["--dry-run", "--out", str(tmp_path), "Hire or outsource?"])
    council._run_and_report(args, "Hire or outsource?", "", council.resolve_callers(args.dry_run))
    sidecars = list(tmp_path.glob("*.json"))
    assert len(sidecars) == 1
    data = council.json.loads(sidecars[0].read_text(encoding="utf-8"))
    assert data["question"] == "Hire or outsource?"
    assert len(data["turns"]) == 5


def test_json_stream_emits_ndjson_events(tmp_path: Path, capsys):
    args = council.build_parser().parse_args(["--dry-run", "--json-stream", "--out", str(tmp_path), "Test question?"])
    council._run_and_report(args, "Test question?", "", council.resolve_callers(args.dry_run))
    lines = capsys.readouterr().out.strip().splitlines()
    events = [council.json.loads(line) for line in lines]
    types = [e["type"] for e in events]
    assert types[0] == "start"
    assert types[-1] == "done"
    assert "turn_start" in types and "turn_complete" in types


def test_rejects_nonpositive_rounds():
    for bad in ("0", "-1"):
        args = council.build_parser().parse_args(["--dry-run", "--rounds", bad, "Hire or outsource?"])
        try:
            council._validate_rounds(args)
        except SystemExit as e:
            assert e.code != 0
        else:
            raise AssertionError(f"--rounds {bad} was accepted")


def test_blank_file_question_exits_cleanly(tmp_path: Path, capsys):
    blank = tmp_path / "blank.md"
    blank.write_text("   \n  \n", encoding="utf-8")
    args = council.build_parser().parse_args(["--dry-run", "--file", str(blank)])
    try:
        council._resolve_question(args)
    except SystemExit as e:
        assert e.code != 0
    else:
        raise AssertionError("blank --file question was accepted")
    assert "Traceback" not in capsys.readouterr().err


def test_missing_file_question_exits_cleanly(tmp_path: Path, capsys):
    missing = tmp_path / "nope.md"
    args = council.build_parser().parse_args(["--dry-run", "--file", str(missing)])
    try:
        council._resolve_question(args)
    except SystemExit as e:
        assert e.code != 0
    else:
        raise AssertionError("missing --file question was accepted")
    assert "Traceback" not in capsys.readouterr().err


def test_prompt_eof_exits_cleanly(monkeypatch, capsys):
    def boom(_prompt: str) -> str:
        raise EOFError

    args = council.build_parser().parse_args(["--dry-run"])
    monkeypatch.setattr("builtins.input", boom)
    try:
        council._resolve_question(args)
    except SystemExit as e:
        assert e.code != 0
    else:
        raise AssertionError("EOF on stdin was accepted")
    assert "Traceback" not in capsys.readouterr().err


def test_json_stream_keeps_stdout_pure_ndjson(tmp_path: Path, capsys):
    ctx = tmp_path / "ctx.md"
    ctx.write_text("budget 5m\n", encoding="utf-8")
    args = council.build_parser().parse_args(
        ["--dry-run", "--json-stream", "--out", str(tmp_path), "--context", str(ctx), "Test question?"]
    )
    council._run_and_report(args, "Test question?", council._build_context(args), council.resolve_callers(args.dry_run))
    out = capsys.readouterr()
    for line in out.out.strip().splitlines():
        council.json.loads(line)
    assert "Included 1 background file(s)" in out.err


def test_report_output_is_owner_only_and_the_html_copy_blocks_scripts(tmp_path: Path):
    turns = council.run_council(session())
    path = council.write_report("Hire or outsource?", turns, tmp_path / "out", rounds=1)
    for written in (path, path.with_suffix(".json"), path.with_suffix(".html")):
        assert written.stat().st_mode & 0o777 == 0o600, written.name
    assert (tmp_path / "out").stat().st_mode & 0o777 == 0o700
    html_copy = path.with_suffix(".html").read_text(encoding="utf-8")
    assert "Content-Security-Policy" in html_copy
    assert "default-src 'none'" in html_copy


def test_export_lines_strip_comments_and_keep_quoted_hashes():
    names = ("COUNCIL_TEST_EXPORT_A", "COUNCIL_TEST_EXPORT_B", "COUNCIL_TEST_EXPORT_C")
    try:
        council._parse_export_line("export COUNCIL_TEST_EXPORT_A=plain # trailing note")
        council._parse_export_line('export COUNCIL_TEST_EXPORT_B="quoted # kept"')
        council._parse_export_line("export COUNCIL_TEST_EXPORT_C")
        council._parse_export_line("export NOT_A NAME=value")
        assert council.os.environ["COUNCIL_TEST_EXPORT_A"] == "plain"
        assert council.os.environ["COUNCIL_TEST_EXPORT_B"] == "quoted # kept"
        assert "COUNCIL_TEST_EXPORT_C" not in council.os.environ
        assert "NOT_A NAME" not in council.os.environ
    finally:
        for name in names:
            council.os.environ.pop(name, None)


@pytest.mark.skipif(not hasattr(os, "getuid"), reason="POSIX mode bits only")
def test_a_world_readable_env_file_warns_before_keys_are_used(tmp_path: Path, capsys):
    env = tmp_path / ".env"
    env.write_text("MISTRAL_API_KEY=fixture\n", encoding="utf-8")
    env.chmod(0o644)
    council._warn_if_world_readable(env)
    assert "chmod 600" in capsys.readouterr().err
    env.chmod(0o600)
    council._warn_if_world_readable(env)
    assert capsys.readouterr().err == ""
