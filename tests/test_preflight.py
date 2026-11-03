"""Input checks, immutable document snapshots, and duplicate-request contracts."""

import io
import json
from types import SimpleNamespace

import pytest

import council


def args_for(*paths):
    args = council.build_parser().parse_args(["--dry-run", "Q?"])
    args.context = [str(p) for p in paths]
    args.context_snapshot = None
    return args


def test_preflight_never_constructs_inference_callers(tmp_path, monkeypatch):
    path = tmp_path / "notes.md"
    path.write_text("Constraint: three engineers.")
    monkeypatch.setattr(council, "resolve_callers", lambda *a: pytest.fail("Inference factory called"))
    report = council._preflight_report(args_for(path))
    assert report["ready"]
    assert report["documents"][0]["snapshot"]["text"] == "Constraint: three engineers."
    assert not (tmp_path / "reports").exists()


def test_missing_and_empty_documents_block_before_inference(tmp_path):
    empty = tmp_path / "empty.txt"
    empty.write_text("  \n")
    report = council._preflight_report(args_for(tmp_path / "missing.pdf", empty))
    assert not report["ready"]
    assert all(document["error"] for document in report["documents"])
    with pytest.raises(ValueError, match="No readable text"):
        council.read_document(empty)


def test_approval_freezes_extracted_text_when_original_changes(tmp_path):
    path = tmp_path / "notes.txt"
    path.write_text("Original decision evidence.")
    args = args_for(path)
    report = council._preflight_report(args)
    args.context_snapshot = [report["documents"][0]["snapshot"]]
    path.write_text("Replacement text that was never reviewed.")
    context = council._build_context(args)
    assert "Original decision evidence." in context
    assert "Replacement" not in context
    assert council._preflight_report(args)["ready"]


def test_tampered_snapshot_cannot_be_executed(tmp_path):
    path = tmp_path / "notes.txt"
    path.write_text("Original.")
    item = council._check_document(path)["snapshot"]
    item["text"] = "Changed without a fresh review."
    with pytest.raises(ValueError, match="changed"):
        council._snapshot_context(item)


def test_truncation_is_explicit_and_only_reviewed_text_is_used(tmp_path):
    path = tmp_path / "long.txt"
    path.write_text("a" * 60_001)
    checked = council._check_document(path)
    assert checked["snapshot"]["original_characters"] == 60_001
    assert len(checked["snapshot"]["text"]) == 60_000
    assert "Only 60000 of 60001" in council._snapshot_context(checked["snapshot"])


def test_missing_credentials_block_real_but_not_practice(monkeypatch):
    for names in council.PROVIDER_ENDPOINTS.values():
        for name in names[1]:
            monkeypatch.delenv(name, raising=False)
    args = args_for()
    assert council._preflight_report(args)["ready"]
    args.dry_run = False
    assert not council._preflight_report(args)["ready"]


def test_request_identity_is_reserved_once_across_process_retries(tmp_path):
    args = SimpleNamespace(request_id="same-approved-plan", out=str(tmp_path))
    council._claim_request(args)
    with pytest.raises(ValueError, match="already attempted"):
        council._claim_request(args)
    assert len(list((tmp_path / ".requests").glob("*.json"))) == 1


def test_stdin_plan_does_not_require_question_in_process_arguments(monkeypatch):
    args = council.build_parser().parse_args(["--config-stdin", "--dry-run"])
    monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps({"question": "Private decision", "rounds": 2})))
    council._load_ui_config(args)
    assert args.question == "Private decision"
    assert args.rounds == 2


def test_legacy_resume_skips_completed_work():
    session = council.Session("Q", "", {p: council.call_fake(p) for p in council.PROVIDER_ORDER})
    council.run_turn(session, "Mistral", "Opening", council.PROPOSER, "Q")
    first = session.turns[0]
    result = council.run_council(session)
    assert len(result) == 6
    assert result[0] is first
    assert result[1].role == "Red team"


def run_stdin(tmp_path, config):
    import subprocess
    import sys

    command = [sys.executable, "council.py", "--config-stdin", "--json-stream", "--out", str(tmp_path)]
    # Fixed argv and a test-owned temporary path: no shell, no caller-supplied program.
    result = subprocess.run(  # noqa: S603
        command, input=json.dumps(config), text=True, capture_output=True, check=False
    )
    return result, [json.loads(line) for line in result.stdout.splitlines()]


def offline_config(request_id):
    return {
        "question": "Offline integration question?",
        "rounds": 1,
        "dry_run": True,
        "request_id": request_id,
        "context_files": [],
        "context_snapshot": [],
        "roles": {},
    }


def test_real_engine_uses_snapshot_after_original_document_is_removed(tmp_path):
    path = tmp_path / "evidence.txt"
    path.write_text("Reviewed material. " * 20_000)
    config = offline_config("snapshot-run")
    config["context_files"] = [str(path)]
    config["context_snapshot"] = [council._check_document(path)["snapshot"]]
    path.unlink()
    result, events = run_stdin(tmp_path, config)
    assert result.returncode == 0, result.stderr
    assert any(event["type"] == "done" for event in events)
    report = next(tmp_path.glob("*.json"))
    assert len(json.loads(report.read_text())["run_config"]["context_snapshot"][0]["text"]) == 60_000


def test_engine_retransmission_cannot_repeat_a_model_stage(tmp_path):
    first, _ = run_stdin(tmp_path, offline_config("one-approval"))
    second, events = run_stdin(tmp_path, offline_config("one-approval"))
    assert first.returncode == 0
    assert second.returncode != 0
    assert not any(event["type"] in ("turn_start", "brief_start") for event in events)
    assert len(list(tmp_path.glob("*.json"))) == 1
    assert "already attempted" in events[-1]["message"]


def test_checkpoint_continuation_only_executes_the_missing_stages(tmp_path):
    first, _ = run_stdin(tmp_path, offline_config("original"))
    assert first.returncode == 0
    saved = json.loads(next(tmp_path.glob("*.json")).read_text())
    config = offline_config("continued")
    config["completed_turns"] = saved["turns"][:2]
    result, events = run_stdin(tmp_path, config)
    assert result.returncode == 0, result.stderr
    starts = [event["index"] for event in events if event["type"] in ("turn_start", "brief_start")]
    assert starts == [3, 4, 5, 6]
    assert any(event["type"] == "done" for event in events)


def write_pdf(path):
    pdf = pytest.importorskip("pypdf")
    generic = pytest.importorskip("pypdf.generic")
    writer = pdf.PdfWriter()
    page = writer.add_blank_page(width=612, height=792)
    page[generic.NameObject("/Resources")] = _pdf_resources(generic)
    stream = generic.DecodedStreamObject()
    stream.set_data(b"BT /F1 12 Tf 72 720 Td (Decision evidence) Tj ET")
    page[generic.NameObject("/Contents")] = stream
    writer.write(path)


def _pdf_resources(generic):
    font = generic.DictionaryObject(
        {
            generic.NameObject("/Type"): generic.NameObject("/Font"),
            generic.NameObject("/Subtype"): generic.NameObject("/Type1"),
            generic.NameObject("/BaseFont"): generic.NameObject("/Helvetica"),
        }
    )
    return generic.DictionaryObject(
        {generic.NameObject("/Font"): generic.DictionaryObject({generic.NameObject("/F1"): font})}
    )


def write_docx(path):
    document = pytest.importorskip("docx").Document()
    document.add_paragraph("Decision evidence")
    document.save(path)


def write_xlsx(path):
    workbook = pytest.importorskip("openpyxl").Workbook()
    workbook.active["A1"] = "Decision evidence"
    workbook.save(path)


@pytest.mark.parametrize(("extension", "writer"), [("pdf", write_pdf), ("docx", write_docx), ("xlsx", write_xlsx)])
def test_optional_document_formats_are_extracted_before_approval(tmp_path, extension, writer):
    path = tmp_path / f"evidence.{extension}"
    writer(path)
    report = council._preflight_report(args_for(path))
    assert report["ready"], report
    assert "Decision evidence" in report["documents"][0]["snapshot"]["text"]


def test_noncontiguous_retained_work_is_rejected_before_execution():
    item = {"index": 2, "model": "Mistral", "role": "Opening", "text": "Saved", "seconds": 1}
    with pytest.raises(ValueError, match="contiguous prefix"):
        council._resume_turns(json.dumps([item]))


def fail_storage(*args, **kwargs):
    raise OSError("Fixture storage unavailable")


def fail_completed_journal(session, event, **fields):
    if event == "stage_completed":
        fail_storage()


@pytest.mark.parametrize("failure", ["checkpoint", "journal"])
def test_completed_argument_reaches_ui_even_when_its_save_fails(tmp_path, monkeypatch, capsys, failure):
    session = council.Session("Q", "", {"Mistral": lambda *a: "Retain this generated argument."})
    session.json_stream, session.checkpoint = True, tmp_path / "run.md"
    if failure == "journal":
        monkeypatch.setattr(council, "_journal_event", fail_completed_journal)
    else:
        monkeypatch.setattr(council, "private_write", fail_storage)
    with pytest.raises(OSError, match="Fixture storage unavailable"):
        council.run_turn(session, "Mistral", "Opening", "System", "Question")
    events = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert any(event["type"] == "turn_complete" and event["text"] == session.turns[0].text for event in events)
    assert not any(event["type"] == "checkpoint_saved" for event in events)


def test_failed_initial_storage_never_starts_inference(tmp_path):
    blocked = tmp_path / "not-a-directory"
    blocked.write_text("Fixture")
    result, events = run_stdin(blocked, offline_config("failed-storage"))
    assert result.returncode != 0
    assert events[-1]["type"] == "error"
    assert not any(event["type"] in ("turn_start", "brief_start", "checkpoint_saved", "done") for event in events)


def named_document_snapshots(tmp_path):
    name = "Planning café & costs #1 + 100%.txt"
    paths = [tmp_path / folder / name for folder in ("first-upload", "second-upload")]
    for index, path in enumerate(paths):
        path.parent.mkdir()
        path.write_text(f"Synthetic source {index + 1}: exact Unicode café.")
    return paths, [council._check_document(path)["snapshot"] for path in paths]


def test_same_original_names_survive_frozen_engine_checkpoint(tmp_path):
    paths, snapshots = named_document_snapshots(tmp_path)
    config = offline_config("original-document-names")
    config.update(context_files=[str(path) for path in paths], context_snapshot=snapshots)
    for path in paths:
        path.unlink()
    result, events = run_stdin(tmp_path, config)
    assert result.returncode == 0, result.stderr
    saved = json.loads(next(tmp_path.glob("*.json")).read_text())
    assert saved["run_config"]["context_snapshot"] == snapshots
    assert [item["name"] for item in snapshots] == [paths[0].name, paths[1].name]
    assert saved["run_config"]["context_files"] == config["context_files"]
    assert any(event["type"] == "done" for event in events)
